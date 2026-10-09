use crate::{
    activity::{self, Progress, Snapshot},
    config::Config,
    content::{self, Content, Role},
    model::{self, Session, safe_text},
    theme::Theme,
};
use anyhow::Result;
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyModifiers,
        MouseEventKind,
    },
    execute,
};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, List, ListItem, ListState, Paragraph, Scrollbar,
        ScrollbarOrientation, ScrollbarState, Wrap,
    },
};
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

#[derive(Clone)]
enum Row {
    Project(String),
    Session(usize),
}
pub struct App {
    sessions: Vec<Session>,
    rows: Vec<Row>,
    state: ListState,
    filter: String,
    editing: bool,
    details_focus: bool,
    detail_scroll: u16,
    help: bool,
    expanded: bool,
    technical: bool,
    notice: String,
    stale: bool,
    busy: bool,
    content_pending: bool,
    jump_pending: bool,
    last_scan: Option<Instant>,
    last_content: Option<Instant>,
    last_activity: Option<Instant>,
    activity_pending: bool,
    activity: BTreeMap<String, Progress>,
    content: Option<(String, Result<Content, String>)>,
    selected_since: Instant,
    dirty: bool,
    list_area: Rect,
    details_area: Rect,
}
impl App {
    fn new() -> Self {
        Self {
            sessions: vec![],
            rows: vec![],
            state: ListState::default(),
            filter: String::new(),
            editing: false,
            details_focus: false,
            detail_scroll: 0,
            help: false,
            expanded: false,
            technical: false,
            notice: "Finding sessions…".into(),
            stale: false,
            busy: false,
            content_pending: false,
            jump_pending: false,
            last_scan: None,
            last_content: None,
            last_activity: None,
            activity_pending: false,
            activity: BTreeMap::new(),
            content: None,
            selected_since: Instant::now(),
            dirty: true,
            list_area: Rect::default(),
            details_area: Rect::default(),
        }
    }
    fn selected(&self) -> Option<&Session> {
        match self.rows.get(self.state.selected()?)? {
            Row::Session(i) => self.sessions.get(*i),
            _ => None,
        }
    }
    fn row_key(&self) -> Option<String> {
        self.selected().map(|s| s.id.clone())
    }
    fn selection_changed(&mut self) {
        self.detail_scroll = 0;
        self.expanded = false;
        self.selected_since = Instant::now();
        self.last_content = None;
        self.content = None;
        self.dirty = true;
    }
    fn rebuild(&mut self, preferred: Option<String>) {
        let old = self.state.selected().unwrap_or(0);
        let needle = self.filter.to_lowercase();
        let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (i, s) in self.sessions.iter().enumerate() {
            let name = s
                .location
                .as_ref()
                .map(|p| format!("{} {} {}", p.name, p.tab, p.window))
                .unwrap_or_default();
            if format!(
                "{} {} {} {}",
                s.provider,
                s.project,
                s.cwd.as_deref().unwrap_or(""),
                name
            )
            .to_lowercase()
            .contains(&needle)
            {
                groups.entry(s.project.clone()).or_default().push(i);
            }
        }
        self.rows.clear();
        for (project, indexes) in groups {
            self.rows.push(Row::Project(project));
            self.rows.extend(indexes.into_iter().map(Row::Session));
        }
        let selected = preferred
            .and_then(|id| {
                self.rows
                    .iter()
                    .position(|r| matches!(r,Row::Session(i) if self.sessions[*i].id==id))
            })
            .or_else(|| {
                self.rows
                    .iter()
                    .enumerate()
                    .filter(|(_, r)| matches!(r, Row::Session(_)))
                    .min_by_key(|(i, _)| i.abs_diff(old))
                    .map(|(i, _)| i)
            });
        self.state.select(selected);
    }
    fn reconcile(&mut self, sessions: Vec<Session>) {
        let key = self.row_key();
        let changed = self.sessions != sessions || self.last_scan.is_none() || self.stale;
        if changed {
            self.sessions = sessions;
            self.activity
                .retain(|id, _| self.sessions.iter().any(|s| &s.id == id));
            if self
                .sessions
                .iter()
                .any(|s| !self.activity.contains_key(&s.id))
            {
                self.last_activity = None;
            }
            self.rebuild(key.clone());
            self.dirty = true;
            if self.row_key() != key {
                self.selection_changed();
            }
        }
        if self.last_scan.is_none() {
            self.notice.clear();
        }
        self.last_scan = Some(Instant::now());
        self.stale = false;
    }
    fn observe(&mut self, snapshot: Snapshot) {
        let before = self.activity.clone();
        for (id, observed) in snapshot.sessions {
            if let Some(session) = self.sessions.iter().find(|s| s.id == id) {
                let focused = session
                    .location
                    .as_ref()
                    .is_some_and(|p| snapshot.focused.as_ref() == Some(&p.id));
                self.activity
                    .entry(id)
                    .or_default()
                    .observe(observed.as_ref(), focused);
            }
        }
        self.dirty |= self.activity != before;
    }
    fn move_selection(&mut self, delta: isize) {
        let selectable: Vec<_> = self
            .rows
            .iter()
            .enumerate()
            .filter(|(_, r)| matches!(r, Row::Session(_)))
            .map(|(i, _)| i)
            .collect();
        if selectable.is_empty() {
            return;
        }
        let old = self.state.selected();
        let index = selectable.iter().position(|i| Some(*i) == old).unwrap_or(0);
        let next = index.saturating_add_signed(delta).min(selectable.len() - 1);
        self.state.select(Some(selectable[next]));
        if self.state.selected() != old {
            self.selection_changed();
        }
    }
}
enum Request {
    Refresh(bool),
    Content(Box<Session>),
    Activity(Vec<Session>),
    Stop,
}
enum Response {
    Refreshed(Result<Vec<Session>>),
    Content(String, Result<Content>),
    Activity(Result<Snapshot>),
    Jumped(String, Result<()>),
}
struct Worker {
    tx: mpsc::Sender<Request>,
    jumps: mpsc::Sender<Option<Session>>,
    rx: mpsc::Receiver<Response>,
    joins: Vec<thread::JoinHandle<()>>,
    cancelled: Arc<AtomicBool>,
}
impl Worker {
    fn start(metadata_seconds: u64) -> Self {
        let (tx, requests) = mpsc::channel();
        let (results, rx) = mpsc::channel();
        let (jumps, jump_requests) = mpsc::channel::<Option<Session>>();
        let cancelled = Arc::new(AtomicBool::new(false));
        let stopping = cancelled.clone();
        let jump_stopping = cancelled.clone();
        let jump_results = results.clone();
        let scan = thread::spawn(move || {
            let mut collector = model::Collector::new(metadata_seconds);
            while let Ok(request) = requests.recv() {
                if stopping.load(Ordering::Relaxed) {
                    break;
                }
                let response = match request {
                    Request::Refresh(force) => Response::Refreshed(collector.scan(force)),
                    Request::Content(s) => {
                        Response::Content(s.id.clone(), content::read(&s, &mut collector.terminal))
                    }
                    Request::Activity(s) => {
                        Response::Activity(activity::read(&s, &mut collector.terminal))
                    }
                    Request::Stop => break,
                };
                if results.send(response).is_err() {
                    break;
                }
            }
        });
        // Navigation never waits behind a metadata/content scan.
        let jump = thread::spawn(move || {
            let mut terminal = crate::iterm::Client::default();
            let _ = terminal.warm();
            while let Ok(Some(s)) = jump_requests.recv() {
                if jump_stopping.load(Ordering::Relaxed) {
                    break;
                }
                if jump_results
                    .send(Response::Jumped(
                        s.id.clone(),
                        model::jump_with(&s, &mut terminal),
                    ))
                    .is_err()
                {
                    break;
                }
            }
        });
        Self {
            tx,
            jumps,
            rx,
            joins: vec![scan, jump],
            cancelled,
        }
    }
    fn refresh(&self, app: &mut App, force: bool) {
        if !app.busy && self.tx.send(Request::Refresh(force)).is_ok() {
            app.busy = true;
        }
    }
    fn jump(&self, app: &mut App) {
        if app.jump_pending {
            return;
        }
        if let Some(s) = app.selected().cloned()
            && self.jumps.send(Some(s)).is_ok()
        {
            app.jump_pending = true;
            app.notice = "Opening session…".into();
            app.dirty = true;
        }
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
        let _ = self.tx.send(Request::Stop);
        let _ = self.jumps.send(None);
        for h in self.joins.drain(..) {
            let _ = h.join();
        }
    }
}
struct TerminalGuard;
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = execute!(std::io::stdout(), DisableMouseCapture);
        ratatui::restore();
    }
}
pub fn run(config: Config, theme: Theme) -> Result<()> {
    let terminate = Arc::new(AtomicBool::new(false));
    let mut signals = vec![];
    for sig in [signal_hook::consts::SIGTERM, signal_hook::consts::SIGINT] {
        signals.push(signal_hook::flag::register(sig, terminate.clone())?);
    }
    let result = (|| -> Result<()> {
        let worker = Worker::start(config.metadata_seconds);
        let mut terminal = ratatui::try_init()?;
        let _guard = TerminalGuard;
        execute!(std::io::stdout(), EnableMouseCapture)?;
        let mut app = App::new();
        worker.refresh(&mut app, true);
        while !terminate.load(Ordering::Relaxed) {
            while let Ok(response) = worker.rx.try_recv() {
                match response {
                    Response::Refreshed(Ok(s)) => {
                        app.busy = false;
                        app.reconcile(s);
                    }
                    Response::Refreshed(Err(e)) => {
                        app.busy = false;
                        let notice = format!("Refresh failed: {e}");
                        if notice != app.notice || !app.stale {
                            app.dirty = true;
                        }
                        app.notice = notice;
                        app.stale = true;
                        app.last_scan = Some(Instant::now());
                    }
                    Response::Content(id, result) => {
                        app.content_pending = false;
                        if app.row_key().as_deref() == Some(id.as_str()) {
                            let next = Some((id, result.map_err(|e| e.to_string())));
                            if next != app.content {
                                app.content = next;
                                app.dirty = true;
                            }
                            app.last_content = Some(Instant::now());
                        }
                    }
                    Response::Activity(result) => {
                        app.activity_pending = false;
                        app.last_activity = Some(Instant::now());
                        match result {
                            Ok(snapshot) => app.observe(snapshot),
                            Err(_) => {
                                let before = app.activity.clone();
                                for progress in app.activity.values_mut() {
                                    progress.observe(None, false);
                                }
                                app.dirty |= app.activity != before;
                            }
                        }
                    }
                    Response::Jumped(id, result) => {
                        app.jump_pending = false;
                        if result.is_ok()
                            && let Some(progress) = app.activity.get_mut(&id)
                        {
                            progress.acknowledge();
                        }
                        app.notice = match result {
                            Ok(()) => "Session opened · Sova stays here".into(),
                            Err(e) => format!("Jump refused: {e}"),
                        };
                        app.dirty = true;
                    }
                }
            }
            if app
                .last_scan
                .is_some_and(|t| t.elapsed() >= Duration::from_secs(config.refresh_seconds))
            {
                worker.refresh(&mut app, false);
            }
            if !app.content_pending
                && app.selected_since.elapsed() >= Duration::from_millis(150)
                && app
                    .last_content
                    .is_none_or(|t| t.elapsed() >= Duration::from_secs(config.content_seconds))
                && let Some(s) = app.selected().cloned()
                && worker.tx.send(Request::Content(Box::new(s))).is_ok()
            {
                app.content_pending = true;
            }
            if !app.activity_pending
                && !app.sessions.is_empty()
                && app
                    .last_activity
                    .is_none_or(|t| t.elapsed() >= Duration::from_secs(config.content_seconds))
                && worker
                    .tx
                    .send(Request::Activity(app.sessions.clone()))
                    .is_ok()
            {
                app.activity_pending = true;
            }
            if app.dirty {
                terminal.draw(|f| draw(f, &mut app, &theme))?;
                app.dirty = false;
            }
            if event::poll(Duration::from_millis(100))? {
                match event::read()? {
                    Event::Key(k) if k.kind == event::KeyEventKind::Press => {
                        app.dirty = true;
                        if key(k, &mut app, &worker) {
                            break;
                        }
                    }
                    Event::Resize(_, _) => app.dirty = true,
                    Event::Mouse(m) => {
                        let details = app.details_area.contains((m.column, m.row).into());
                        let list = app.list_area.contains((m.column, m.row).into());
                        match m.kind {
                            MouseEventKind::ScrollDown | MouseEventKind::ScrollUp
                                if details || list =>
                            {
                                app.details_focus = details;
                                let down = m.kind == MouseEventKind::ScrollDown;
                                if details {
                                    app.detail_scroll = if down {
                                        app.detail_scroll.saturating_add(3)
                                    } else {
                                        app.detail_scroll.saturating_sub(3)
                                    };
                                } else {
                                    app.move_selection(if down { 1 } else { -1 });
                                }
                                app.dirty = true;
                            }
                            MouseEventKind::Down(event::MouseButton::Left) if details => {
                                app.details_focus = true;
                                app.dirty = true;
                            }
                            MouseEventKind::Down(event::MouseButton::Left) if list => {
                                let row = app.state.offset()
                                    + m.row.saturating_sub(app.list_area.y + 1) as usize;
                                if matches!(app.rows.get(row), Some(Row::Session(_))) {
                                    app.state.select(Some(row));
                                    app.selection_changed();
                                }
                                app.details_focus = false;
                                app.dirty = true;
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    })();
    for id in signals {
        signal_hook::low_level::unregister(id);
    }
    result
}
fn key(k: KeyEvent, app: &mut App, worker: &Worker) -> bool {
    if k.modifiers.contains(KeyModifiers::CONTROL) && k.code == KeyCode::Char('c') {
        return true;
    }
    if app.editing {
        let old = app.row_key();
        match k.code {
            KeyCode::Esc => {
                app.editing = false;
                app.filter.clear();
            }
            KeyCode::Enter => app.editing = false,
            KeyCode::Backspace => {
                app.filter.pop();
            }
            KeyCode::Char(c) => app.filter.push(c),
            _ => {}
        }
        app.rebuild(old.clone());
        if app.row_key() != old {
            app.selection_changed();
        }
        return false;
    }
    if app.help {
        match k.code {
            KeyCode::Char('q' | 'й') => return true,
            KeyCode::Esc | KeyCode::Char('?') => app.help = false,
            _ => {}
        }
        return false;
    }
    match k.code {
        KeyCode::Char('q' | 'й') | KeyCode::Esc => return true,
        KeyCode::Char('/') => app.editing = true,
        KeyCode::Char('?') => app.help = true,
        KeyCode::Tab => app.details_focus = !app.details_focus,
        KeyCode::Right => app.details_focus = true,
        KeyCode::BackTab | KeyCode::Left => app.details_focus = false,
        KeyCode::Char('r' | 'к') => {
            worker.refresh(app, true);
            app.last_content = None;
            app.last_activity = None;
        }
        KeyCode::Char('o' | 'щ') | KeyCode::Enter => worker.jump(app),
        KeyCode::Char('e' | 'у') => {
            app.expanded = !app.expanded;
            app.detail_scroll = 0;
        }
        KeyCode::Char('m' | 'ь') => app.technical = !app.technical,
        KeyCode::Up | KeyCode::Char('k' | 'л') => {
            if app.details_focus {
                app.detail_scroll = app.detail_scroll.saturating_sub(1);
            } else {
                app.move_selection(-1);
            }
        }
        KeyCode::Down | KeyCode::Char('j' | 'о') => {
            if app.details_focus {
                app.detail_scroll = app.detail_scroll.saturating_add(1);
            } else {
                app.move_selection(1);
            }
        }
        KeyCode::PageUp => {
            if app.details_focus {
                app.detail_scroll = app.detail_scroll.saturating_sub(10);
            } else {
                app.move_selection(-10);
            }
        }
        KeyCode::PageDown => {
            if app.details_focus {
                app.detail_scroll = app.detail_scroll.saturating_add(10);
            } else {
                app.move_selection(10);
            }
        }
        KeyCode::Home => {
            if app.details_focus {
                app.detail_scroll = 0;
            } else {
                app.move_selection(isize::MIN);
            }
        }
        KeyCode::End => {
            if app.details_focus {
                app.detail_scroll = u16::MAX;
            } else {
                app.move_selection(isize::MAX);
            }
        }
        _ => {}
    }
    false
}
fn heading(name: &str, t: &Theme) -> Line<'static> {
    Line::styled(
        format!(" {name} "),
        Style::default().fg(t.label).add_modifier(Modifier::BOLD),
    )
}
fn field(label: &str, value: String, t: &Theme) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label}  "), Style::default().fg(t.label)),
        Span::styled(safe_text(&value), Style::default().fg(t.task)),
    ])
}
fn preview(text: &str, expanded: bool) -> String {
    if expanded {
        return text.to_owned();
    }
    let lines: Vec<_> = text.lines().take(4).collect();
    let short = lines.join("\n");
    let truncated = text.lines().count() > 4 || short.chars().count() > 280;
    let short: String = short.chars().take(280).collect();
    if truncated {
        format!("{short}\n… e to expand")
    } else {
        short
    }
}
fn body(lines: &mut Vec<Line<'static>>, text: &str, t: &Theme) {
    for line in text.lines() {
        lines.push(Line::styled(safe_text(line), Style::default().fg(t.task)));
    }
}
fn details(app: &App, t: &Theme) -> Vec<Line<'static>> {
    let Some(s) = app.selected() else {
        return vec![Line::raw("No matching sessions")];
    };
    let content = app
        .content
        .as_ref()
        .filter(|(id, _)| id == &s.id)
        .map(|(_, c)| c);
    let state = match app.activity.get(&s.id) {
        Some(progress) => progress.explanation(),
        None => match content {
            Some(Ok(c)) => c.state,
            _ => "Live · activity unknown",
        },
    };
    let mut lines = vec![
        heading("STATUS", t),
        Line::from(vec![
            Span::styled(
                format!("{}  ", s.provider),
                Style::default().fg(t.task).add_modifier(Modifier::BOLD),
            ),
            Span::styled(state, Style::default().fg(t.info)),
        ]),
        Line::default(),
        heading("TERMINAL & SESSION", t),
    ];
    if let Some(progress) = app.activity.get(&s.id) {
        lines.insert(2, field("Opening", progress.opening().into(), t));
    }
    if let Some(p) = &s.location {
        lines.push(field(
            "iTerm2",
            format!(
                "{} · tab {} · pane {}",
                p.window,
                p.tab_index + 1,
                p.pane_index + 1
            ),
            t,
        ));
    } else {
        lines.push(field(
            "Jump unavailable",
            s.link_reason.clone().unwrap_or_default(),
            t,
        ));
    }
    lines.push(field(
        "Folder",
        s.cwd.clone().unwrap_or_else(|| "Unknown".into()),
        t,
    ));
    if s.cwd.as_deref() != Some(&s.project) {
        lines.push(field("Git root", s.project.clone(), t));
    }
    lines.push(Line::default());
    lines.push(heading("LATEST EXCHANGE", t));
    match content {
        Some(Ok(c)) => {
            for role in [Role::You, Role::Agent] {
                lines.push(field(role.label(), String::new(), t));
                if let Some(m) = c.messages.iter().rev().find(|m| m.role == role) {
                    body(&mut lines, &preview(&m.text, app.expanded), t);
                } else {
                    lines.push(Line::styled(
                        "Not visible in terminal history",
                        Style::default().fg(t.status),
                    ));
                }
                lines.push(Line::default());
            }
            lines.push(heading("RECENT HISTORY", t));
            lines.push(Line::styled(
                "Read-only terminal snapshot · may be incomplete",
                Style::default().fg(t.status),
            ));
            for m in &c.messages {
                lines.push(Line::default());
                lines.push(Line::styled(
                    m.role.label(),
                    Style::default()
                        .fg(if m.role == Role::You {
                            t.project
                        } else {
                            t.label
                        })
                        .add_modifier(Modifier::BOLD),
                ));
                body(&mut lines, &preview(&m.text, app.expanded), t);
            }
            if c.messages.is_empty() {
                body(&mut lines, &preview(&c.context, app.expanded), t);
            }
        }
        Some(Err(e)) => body(&mut lines, &format!("Content unavailable: {e}"), t),
        None => lines.push(Line::styled(
            "Reading selected terminal…",
            Style::default().fg(t.status),
        )),
    }
    if app.technical {
        lines.push(Line::default());
        lines.push(heading("TECHNICAL DETAILS", t));
        lines.push(field(
            "Process",
            format!("{} · started {} µs", s.pid, s.started),
            t,
        ));
        lines.push(field("Session", s.id.clone(), t));
        if let Some(p) = &s.location {
            lines.push(field("Pane UUID", p.id.clone(), t));
            lines.push(field("TTY", p.tty.clone(), t));
        }
    }
    lines
}
fn session_title(s: &Session) -> String {
    let mut title = s
        .location
        .as_ref()
        .map(|p| p.name.clone())
        .unwrap_or_else(|| "Terminal unavailable".into());
    title = title
        .trim_start_matches(|c: char| c.is_whitespace() || ('⠀'..='⣿').contains(&c))
        .to_owned();
    for suffix in [" (codex)", " (claude)"] {
        if let Some(v) = title.strip_suffix(suffix) {
            title = v.to_owned();
        }
    }
    let project = std::path::Path::new(&s.project)
        .file_name()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    if let Some(v) = title.strip_suffix(&format!(" | {project}")) {
        title = v.to_owned();
    }
    safe_text(&title)
}
fn draw(f: &mut Frame, app: &mut App, t: &Theme) {
    let area = f.area();
    if area.width < 50 || area.height < 10 {
        f.render_widget(
            Paragraph::new(format!(
                "Sova v{} · resize to 50 × 10 · q exit",
                env!("CARGO_PKG_VERSION")
            )),
            area,
        );
        return;
    }
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(1),
            Constraint::Length(2),
        ])
        .split(area);
    let filter = if app.editing || !app.filter.is_empty() {
        format!(
            "  / {}{}",
            safe_text(&app.filter),
            if app.editing { "_" } else { "" }
        )
    } else {
        String::new()
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!("Sova v{}", env!("CARGO_PKG_VERSION")),
                Style::default().fg(t.task).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(
                    "  ·  {} sessions{}{filter}",
                    app.sessions.len(),
                    if app.stale { "  ·  STALE" } else { "" }
                ),
                Style::default().fg(t.status),
            ),
        ])),
        outer[0],
    );
    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(outer[1]);
    app.list_area = panes[0];
    app.details_area = panes[1];
    let selected = app.state.selected();
    let items: Vec<_> = app
        .rows
        .iter()
        .enumerate()
        .map(|(row_index, row)| match row {
            Row::Project(p) => {
                let name = std::path::Path::new(p)
                    .file_name()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|| p.clone());
                ListItem::new(Line::styled(
                    format!(" {name}"),
                    Style::default().fg(t.project).add_modifier(Modifier::BOLD),
                ))
            }
            Row::Session(i) => {
                let s = &app.sessions[*i];
                let active = selected == Some(row_index);
                let progress = app.activity.get(&s.id).cloned().unwrap_or_default();
                let color = match progress.label() {
                    "Approval" | "Blocked" | "Question" => t.error,
                    "Working" => t.info,
                    "Ready" => t.project,
                    _ => t.status,
                };
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!(" {} ", progress.symbol()),
                        Style::default().fg(if active { t.selected_fg } else { color }),
                    ),
                    Span::styled(
                        format!("{:<7} ", s.provider.to_string()),
                        Style::default().fg(if active { t.selected_fg } else { t.status }),
                    ),
                    Span::styled(
                        session_title(s),
                        Style::default().fg(if active { t.selected_fg } else { t.task }),
                    ),
                ]))
            }
        })
        .collect();
    let border = |active: bool| Style::default().fg(if active { t.list_border } else { t.status });
    let block = |title: &str, active: bool| {
        Block::default()
            .title(title.to_owned())
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(if active && app.details_focus {
                Style::default().fg(t.detail_border)
            } else {
                border(active)
            })
    };
    f.render_stateful_widget(
        List::new(items)
            .block(block(" Sessions ", !app.details_focus))
            .highlight_style(
                Style::default()
                    .bg(t.selected_bg)
                    .fg(t.selected_fg)
                    .add_modifier(Modifier::BOLD),
            )
            .scroll_padding(2),
        panes[0],
        &mut app.state,
    );
    if app.rows.is_empty() {
        f.render_widget(
            Paragraph::new(if app.last_scan.is_none() {
                "Finding sessions…"
            } else if app.filter.is_empty() {
                "No live sessions"
            } else {
                "No matching sessions"
            })
            .block(block(" Sessions ", !app.details_focus)),
            panes[0],
        );
    }
    let paragraph = Paragraph::new(details(app, t)).wrap(Wrap { trim: false });
    let total = paragraph.line_count(panes[1].width.saturating_sub(2).max(1));
    let visible = panes[1].height.saturating_sub(2) as usize;
    let max_scroll = total.saturating_sub(visible).min(u16::MAX as usize) as u16;
    app.detail_scroll = app.detail_scroll.min(max_scroll);
    let title = if max_scroll > 0 {
        format!(
            " Details · {}/{} · wheel or Tab + ↑↓ ",
            app.detail_scroll + 1,
            max_scroll + 1
        )
    } else {
        " Details ".into()
    };
    f.render_widget(
        paragraph
            .block(block(&title, app.details_focus))
            .scroll((app.detail_scroll, 0)),
        panes[1],
    );
    if max_scroll > 0 {
        let mut state = ScrollbarState::new(total)
            .viewport_content_length(visible)
            .position(app.detail_scroll as usize);
        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight),
            panes[1],
            &mut state,
        );
    }
    let expansion = if app.expanded {
        "e collapse"
    } else {
        "e expand"
    };
    let help = if app.details_focus {
        format!("↑↓ scroll details · Tab sessions · {expansion} · m metadata · q quit")
    } else {
        format!("↑↓ sessions · Enter Jump · Tab details · / search · {expansion} · ? help · q quit")
    };
    f.render_widget(
        Paragraph::new(vec![
            Line::styled(help, Style::default().fg(t.status)),
            Line::styled(
                safe_text(&app.notice),
                Style::default().fg(if app.stale || app.notice.contains("refused") {
                    t.error
                } else {
                    t.info
                }),
            ),
        ]),
        outer[2],
    );
    if app.help {
        f.render_widget(ratatui::widgets::Clear, panes[1]);
        f.render_widget(Paragraph::new("↑/↓ or j/k: select sessions (headings are skipped)\nEnter/o: open exact existing pane\nTab: switch pane · →: details · ←: sessions\nWheel: scroll the pane under the pointer\nPageUp/PageDown, Home/End: scroll focused pane\ne: expand/collapse long messages\nm: show/hide technical identifiers\n/: search · r: refresh locations and content\n?: close help · q/Ctrl+C: exit\n\n● Working · ○ Idle · ! Approval\n? Question/action · × Blocked\n◆ New response ready · ✓ Pane opened\n· Activity unknown\nOpening a pane does not prove reading.\nTracking lasts while Sova is open.\n\nContent comes from the selected terminal's visible screen.\nIt may be incomplete; an input draft is not a sent request.\nSova never types into or controls agents.").block(block(" Help ",true)).wrap(Wrap{trim:false}),panes[1]);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn session(id: &str, project: &str) -> Session {
        Session {
            id: id.into(),
            pid: 1,
            started: 1,
            provider: crate::process::Provider::Codex,
            cwd: Some(project.into()),
            project: project.into(),
            status: "Live",
            location: None,
            link_reason: Some("Unknown".into()),
        }
    }
    #[test]
    fn navigation_skips_group_headers_across_refresh_filter_and_pages() {
        let mut app = App::new();
        app.reconcile(vec![
            session("a", "/alpha"),
            session("b", "/beta"),
            session("c", "/beta"),
        ]);
        assert_eq!(app.selected().unwrap().id, "a");
        app.move_selection(1);
        assert_eq!(app.selected().unwrap().id, "b");
        app.detail_scroll = 3;
        app.reconcile(vec![
            session("b", "/beta"),
            session("a", "/alpha"),
            session("c", "/beta"),
        ]);
        assert_eq!(app.selected().unwrap().id, "b");
        assert_eq!(app.detail_scroll, 3);
        app.move_selection(isize::MAX);
        assert_eq!(app.selected().unwrap().id, "c");
        app.move_selection(isize::MIN);
        assert_eq!(app.selected().unwrap().id, "a");
        app.filter = "beta".into();
        app.rebuild(app.row_key());
        assert!(app.selected().is_some());
        app.reconcile(vec![]);
        assert!(app.selected().is_none());
    }
    #[test]
    fn status_symbols_follow_identity_and_selection_does_not_mark_a_response_opened() {
        use crate::activity::{Attention, Observation, State};
        let mut app = App::new();
        app.reconcile(vec![session("a", "/alpha"), session("b", "/alpha")]);
        let snapshot = |state, answer| Snapshot {
            sessions: vec![(
                "a".into(),
                Some(Observation {
                    state,
                    answer,
                    attention: Attention::None,
                }),
            )],
            focused: None,
        };
        app.observe(snapshot(State::Working, None));
        app.observe(snapshot(State::Idle, Some(1)));
        assert_eq!(app.activity["a"].symbol(), "◆");
        app.move_selection(1);
        app.move_selection(-1);
        assert_eq!(app.activity["a"].symbol(), "◆");
        app.dirty = false;
        app.observe(snapshot(State::Idle, Some(1)));
        assert!(!app.dirty);
        let mut term = ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 25)).unwrap();
        term.draw(|f| draw(f, &mut app, &crate::theme::get("classic").unwrap()))
            .unwrap();
        let buffer = term.backend().buffer();
        let sidebar = (app.list_area.y..app.list_area.bottom())
            .map(|y| {
                (0..app.list_area.right())
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(sidebar.contains("◆ Codex"));
        assert!(!sidebar.contains("Ready"));
        app.reconcile(vec![
            session("b", "/alpha"),
            session("new-pid-start", "/alpha"),
        ]);
        assert!(!app.activity.contains_key("a"));
        assert!(!app.activity.contains_key("new-pid-start"));
    }
    #[test]
    fn unchanged_refresh_does_not_redraw_or_clear_notices() {
        let mut app = App::new();
        app.reconcile(vec![session("a", "/alpha")]);
        app.dirty = false;
        app.notice = "Session opened".into();
        app.reconcile(vec![session("a", "/alpha")]);
        assert!(!app.dirty);
        assert_eq!(app.notice, "Session opened");
    }
    #[test]
    fn details_are_structured_and_selected_text_has_contrast() {
        let mut app = App::new();
        app.reconcile(vec![session("a", "/alpha")]);
        app.content = Some((
            "a".into(),
            Ok(content::parse(
                "› Request\n• Answer\n› ",
                crate::process::Provider::Codex,
            )),
        ));
        let mut term = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 30)).unwrap();
        let t = crate::theme::get("classic").unwrap();
        term.draw(|f| draw(f, &mut app, &t)).unwrap();
        let buffer = term.backend().buffer();
        let text = buffer
            .content
            .iter()
            .map(|c| c.symbol())
            .collect::<String>();
        for expected in [
            &format!("Sova v{}", env!("CARGO_PKG_VERSION")),
            "STATUS",
            "TERMINAL & SESSION",
            "LATEST EXCHANGE",
            "Request",
            "Answer",
        ] {
            assert!(text.contains(expected), "{expected}");
        }
        let selected: Vec<_> = buffer
            .content
            .iter()
            .filter(|c| c.bg == t.selected_bg && c.symbol() != " ")
            .collect();
        assert!(!selected.is_empty());
        assert!(selected.iter().all(|c| c.fg == t.selected_fg));
        assert!(!text.contains("Git root"));
        assert!(!text.contains("Pane UUID"));
    }
    #[test]
    fn render_small_empty_and_long_details_with_bounded_scroll() {
        for (w, h) in [(20, 5), (100, 25)] {
            let mut term =
                ratatui::Terminal::new(ratatui::backend::TestBackend::new(w, h)).unwrap();
            let mut app = App::new();
            term.draw(|f| draw(f, &mut app, &crate::theme::get("classic").unwrap()))
                .unwrap();
        }
        let mut app = App::new();
        app.reconcile(vec![session("a", "/alpha")]);
        app.content = Some((
            "a".into(),
            Ok(Content {
                state: "Working",
                messages: vec![content::Message {
                    role: Role::You,
                    text: "long text\n".repeat(100),
                }],
                context: String::new(),
            }),
        ));
        app.expanded = true;
        app.detail_scroll = u16::MAX;
        let mut term = ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 25)).unwrap();
        term.draw(|f| draw(f, &mut app, &crate::theme::get("classic").unwrap()))
            .unwrap();
        assert!(app.detail_scroll > 0 && app.detail_scroll < u16::MAX);
    }
}
