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
    layout::{Constraint, Layout, Rect},
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Focus {
    #[default]
    Sessions,
    History,
    Details,
}
impl Focus {
    fn next(self, backwards: bool) -> Self {
        match (self, backwards) {
            (Self::Sessions, false) | (Self::Details, true) => Self::History,
            (Self::History, false) | (Self::Sessions, true) => Self::Details,
            _ => Self::Sessions,
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::Sessions => "Sessions",
            Self::History => "History",
            Self::Details => "Details",
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum HistoryMode {
    #[default]
    All,
    Answers,
    Questions,
}
impl HistoryMode {
    fn includes(self, role: &Role) -> bool {
        match self {
            Self::All => true,
            Self::Answers => *role == Role::Agent,
            Self::Questions => *role == Role::You,
        }
    }
    fn next(self) -> Self {
        match self {
            Self::All => Self::Answers,
            Self::Answers => Self::Questions,
            Self::Questions => Self::All,
        }
    }
}
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
    focus: Focus,
    history_scroll: u16,
    history_mode: HistoryMode,
    history_area: Rect,
    detail_scroll: u16,
    help: bool,
    expanded: bool,
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
            focus: Focus::Sessions,
            history_scroll: 0,
            history_mode: HistoryMode::All,
            history_area: Rect::default(),
            detail_scroll: 0,
            help: false,
            expanded: false,
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
        self.history_scroll = 0;
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
pub fn run(config: Config, mut theme: Theme) -> Result<()> {
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
                        if key(k, &mut app, &worker, &mut theme) {
                            break;
                        }
                    }
                    Event::Resize(_, _) => app.dirty = true,
                    Event::Mouse(m) => mouse(m, &mut app),
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
fn mouse(m: event::MouseEvent, app: &mut App) {
    if app.help {
        return;
    }
    let focus = if app.history_area.contains((m.column, m.row).into()) {
        Some(Focus::History)
    } else if app.details_area.contains((m.column, m.row).into()) {
        Some(Focus::Details)
    } else if app.list_area.contains((m.column, m.row).into()) {
        Some(Focus::Sessions)
    } else {
        None
    };
    let Some(focus) = focus else {
        return;
    };
    match m.kind {
        MouseEventKind::ScrollDown | MouseEventKind::ScrollUp => {
            app.focus = focus;
            navigate(
                app,
                if m.kind == MouseEventKind::ScrollDown {
                    3
                } else {
                    -3
                },
            );
        }
        MouseEventKind::Down(event::MouseButton::Left) => {
            app.focus = focus;
            if focus == Focus::Sessions
                && m.row > app.list_area.y
                && m.row < app.list_area.bottom().saturating_sub(1)
            {
                let row = app.state.offset() + m.row.saturating_sub(app.list_area.y + 1) as usize;
                if matches!(app.rows.get(row), Some(Row::Session(_)))
                    && app.state.selected() != Some(row)
                {
                    app.state.select(Some(row));
                    app.selection_changed();
                }
            }
            if focus == Focus::History && m.row == app.history_area.y + 1 {
                let x = m.column.saturating_sub(app.history_area.x + 1);
                match x {
                    0..=6 => set_mode(app, HistoryMode::All),
                    8..=18 => set_mode(app, HistoryMode::Answers),
                    20..=32 => set_mode(app, HistoryMode::Questions),
                    _ => {}
                }
            }
        }
        _ => return,
    }
    app.dirty = true;
}
fn navigate(app: &mut App, delta: isize) {
    match app.focus {
        Focus::Sessions => app.move_selection(delta),
        Focus::History => {
            app.history_scroll = app
                .history_scroll
                .saturating_add_signed(delta.clamp(i16::MIN as isize, i16::MAX as isize) as i16)
        }
        Focus::Details => {
            app.detail_scroll = app
                .detail_scroll
                .saturating_add_signed(delta.clamp(i16::MIN as isize, i16::MAX as isize) as i16)
        }
    }
}
fn set_mode(app: &mut App, mode: HistoryMode) {
    app.history_mode = mode;
    app.history_scroll = 0;
    app.focus = Focus::History;
}
fn selected_content(app: &App) -> Option<&Result<Content, String>> {
    let id = &app.selected()?.id;
    app.content
        .as_ref()
        .filter(|(key, _)| key == id)
        .map(|(_, c)| c)
}
fn expandable(app: &App) -> bool {
    match selected_content(app) {
        Some(Ok(c)) => {
            let texts: Vec<&str> = if c.messages.is_empty() && app.history_mode == HistoryMode::All
            {
                vec![&c.context]
            } else {
                c.messages
                    .iter()
                    .filter(|m| app.history_mode.includes(&m.role))
                    .map(|m| m.text.as_str())
                    .collect()
            };
            texts
                .iter()
                .any(|text| text.lines().count() > 4 || text.chars().count() > 280)
        }
        _ => false,
    }
}
fn expand(app: &mut App, expanded: bool) {
    app.focus = Focus::History;
    app.history_scroll = 0;
    if expandable(app) {
        app.expanded = expanded;
        app.notice = if expanded {
            "History: full available text. Use ↑/↓ or wheel to scroll."
        } else {
            "History: message previews. → shows full text."
        }
        .into();
    } else {
        app.expanded = false;
        app.notice = "All available text already fits; there is nothing to expand.".into();
    }
}
fn key(k: KeyEvent, app: &mut App, worker: &Worker, theme: &mut Theme) -> bool {
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
        KeyCode::Tab => app.focus = app.focus.next(false),
        KeyCode::BackTab => app.focus = app.focus.next(true),
        KeyCode::Right => expand(app, true),
        KeyCode::Left => expand(app, false),
        KeyCode::Char('e' | 'у') => expand(app, !app.expanded),
        KeyCode::Char('h' | 'р') => set_mode(app, app.history_mode.next()),
        KeyCode::Char('1') => set_mode(app, HistoryMode::All),
        KeyCode::Char('2') => set_mode(app, HistoryMode::Answers),
        KeyCode::Char('3') => set_mode(app, HistoryMode::Questions),
        KeyCode::Char('t' | 'е') => {
            let index = crate::theme::NAMES
                .iter()
                .position(|name| *name == theme.name)
                .unwrap_or(0);
            let name = crate::theme::NAMES[(index + 1) % crate::theme::NAMES.len()];
            match crate::config::save_theme(name).and_then(|()| crate::theme::get(name)) {
                Ok(next) => {
                    *theme = next;
                    app.notice = format!("Theme: {name} · t chooses the next palette");
                }
                Err(e) => app.notice = format!("Theme change failed: {e}"),
            }
        }
        KeyCode::Char('r' | 'к') => {
            worker.refresh(app, true);
            app.last_content = None;
            app.last_activity = None;
        }
        KeyCode::Char('o' | 'щ') | KeyCode::Enter => worker.jump(app),
        KeyCode::Up | KeyCode::Char('k' | 'л') => navigate(app, -1),
        KeyCode::Down | KeyCode::Char('j' | 'о') => navigate(app, 1),
        KeyCode::PageUp => navigate(app, -10),
        KeyCode::PageDown => navigate(app, 10),
        KeyCode::Home => match app.focus {
            Focus::Sessions => app.move_selection(isize::MIN),
            Focus::History => app.history_scroll = 0,
            Focus::Details => app.detail_scroll = 0,
        },
        KeyCode::End => match app.focus {
            Focus::Sessions => app.move_selection(isize::MAX),
            Focus::History => app.history_scroll = u16::MAX,
            Focus::Details => app.detail_scroll = u16::MAX,
        },
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
        format!("{short}\n… → full text in History")
    } else {
        short
    }
}
fn body(lines: &mut Vec<Line<'static>>, text: &str, t: &Theme) {
    for line in text.lines() {
        lines.push(Line::styled(safe_text(line), Style::default().fg(t.task)));
    }
}
fn separator(name: &str, width: u16, t: &Theme) -> Line<'static> {
    let label = format!("─ {name} ");
    Line::styled(
        format!(
            "{label}{}",
            "─".repeat((width as usize).saturating_sub(label.chars().count()))
        ),
        Style::default().fg(t.label).add_modifier(Modifier::BOLD),
    )
}
fn details(app: &App, t: &Theme) -> Vec<Line<'static>> {
    let Some(s) = app.selected() else {
        return vec![Line::raw("Select a session to see its details")];
    };
    let width = app.details_area.width.saturating_sub(2);
    let mut lines = vec![
        separator("SESSION", width, t),
        field("Name", session_title(s), t),
        field("Agent", s.provider.to_string(), t),
    ];
    if let Some(progress) = app.activity.get(&s.id) {
        lines.push(field("Activity", progress.explanation().into(), t));
        lines.push(field("Response", progress.opening().into(), t));
    }
    lines.push(Line::default());
    lines.push(separator("TERMINAL", width, t));
    if let Some(p) = &s.location {
        lines.push(field("Window", p.window.clone(), t));
        lines.push(field("Tab", format!("{} · {}", p.tab_index + 1, p.tab), t));
        lines.push(field(
            "Pane",
            format!("{} · {}", p.pane_index + 1, p.name),
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
    lines.push(separator("LATEST EXCHANGE", width, t));
    match selected_content(app) {
        Some(Ok(c)) => {
            for role in [Role::You, Role::Agent] {
                lines.push(heading(role.label(), t));
                if let Some(m) = c.messages.iter().rev().find(|m| m.role == role) {
                    body(&mut lines, &preview(&m.text, false), t);
                } else {
                    lines.push(Line::styled(
                        "Not in visible snapshot",
                        Style::default().fg(t.status),
                    ));
                }
                lines.push(Line::default());
            }
        }
        Some(Err(e)) => body(&mut lines, &format!("Content unavailable: {e}"), t),
        None => body(&mut lines, "Reading selected terminal…", t),
    }
    lines.push(separator("IDENTITY", width, t));
    lines.push(field("PID", s.pid.to_string(), t));
    lines.push(field("Session ID", s.id.clone(), t));
    if let Some(p) = &s.location {
        lines.push(field("TTY", p.tty.clone(), t));
        lines.push(field("Pane UUID", p.id.clone(), t));
    }
    lines
}
fn compact_path(path: &str) -> String {
    compact_path_with_home(path, std::env::var("HOME").ok().as_deref())
}
fn compact_path_with_home(path: &str, home: Option<&str>) -> String {
    if home == Some(path) {
        return "~/".into();
    }
    let parts: Vec<_> = std::path::Path::new(path)
        .components()
        .filter_map(|p| match p {
            std::path::Component::Normal(p) => Some(p.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    if let Some(home) = home
        && let Some(relative) = path.strip_prefix(&format!("{home}/"))
        && !relative.is_empty()
        && !relative.contains('/')
    {
        return format!("~/{relative}/");
    }
    match parts.as_slice() {
        [] => path.into(),
        [name] => format!("/{name}/"),
        [.., parent, name] => format!("…/{parent}/{name}/"),
    }
}
fn history(app: &App, t: &Theme) -> Vec<Line<'static>> {
    let Some(_) = app.selected() else {
        return vec![Line::raw("Select a session to see its history")];
    };
    let mut lines = vec![Line::styled(
        "Visible terminal snapshot · older turns may be absent",
        Style::default().fg(t.status),
    )];
    match selected_content(app) {
        Some(Ok(c)) => {
            let messages: Vec<_> = c
                .messages
                .iter()
                .filter(|m| app.history_mode.includes(&m.role))
                .collect();
            if messages.is_empty() {
                if app.history_mode == HistoryMode::All && !c.context.is_empty() {
                    body(&mut lines, &preview(&c.context, app.expanded), t);
                } else {
                    body(
                        &mut lines,
                        "No matching messages in the visible snapshot.",
                        t,
                    );
                }
            }
            for m in messages {
                lines.push(Line::default());
                lines.push(separator(
                    m.role.label(),
                    app.history_area.width.saturating_sub(2),
                    t,
                ));
                body(&mut lines, &preview(&m.text, app.expanded), t);
            }
        }
        Some(Err(e)) => body(&mut lines, &format!("Content unavailable: {e}"), t),
        None => body(&mut lines, "Reading selected terminal…", t),
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
fn pane_block(title: &str, focus: Focus, app: &App, t: &Theme) -> Block<'static> {
    let active = app.focus == focus;
    Block::default()
        .title(format!(" {}{title} ", if active { "▶ " } else { "" }))
        .borders(Borders::ALL)
        .border_type(if active {
            BorderType::Double
        } else {
            BorderType::Rounded
        })
        .border_style(Style::default().fg(if active {
            if focus == Focus::Details {
                t.detail_border
            } else {
                t.list_border
            }
        } else {
            t.status
        }))
        .title_style(if active {
            Style::default()
                .bg(t.selected_bg)
                .fg(t.selected_fg)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(t.status)
        })
}
fn scroll_body(f: &mut Frame, area: Rect, lines: Vec<Line<'static>>, scroll: &mut u16) {
    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
    let total = paragraph.line_count(area.width.max(1));
    let visible = area.height as usize;
    *scroll = (*scroll).min(total.saturating_sub(visible).min(u16::MAX as usize) as u16);
    f.render_widget(paragraph.scroll((*scroll, 0)), area);
    if total > visible {
        let mut state = ScrollbarState::new(total)
            .viewport_content_length(visible)
            .position(*scroll as usize);
        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight),
            Rect {
                width: area.width + 1,
                ..area
            },
            &mut state,
        );
    }
}
fn scroll_pane(
    f: &mut Frame,
    area: Rect,
    lines: Vec<Line<'static>>,
    block: Block<'static>,
    scroll: &mut u16,
) {
    let inner = block.inner(area);
    f.render_widget(block, area);
    scroll_body(f, inner, lines, scroll);
}
fn draw(f: &mut Frame, app: &mut App, t: &Theme) {
    let area = f.area();
    if area.width < 50 || area.height < 12 {
        f.render_widget(
            Paragraph::new(format!(
                "Sova v{} · resize to 50 × 12 · q exit",
                env!("CARGO_PKG_VERSION")
            )),
            area,
        );
        return;
    }
    let outer = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(2),
    ])
    .split(area);
    let filter = if app.editing || !app.filter.is_empty() {
        format!(
            " · / {}{}",
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
                    " · {} · {} sessions{}{filter}",
                    t.name,
                    app.sessions.len(),
                    if app.stale { " · STALE" } else { "" }
                ),
                Style::default().fg(t.status),
            ),
        ])),
        outer[0],
    );
    if area.width >= 90 {
        let columns = Layout::horizontal([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(outer[1]);
        let desired = (app.rows.len() as u16)
            .saturating_add(2)
            .max(5)
            .min(columns[0].height.saturating_mul(2) / 5)
            .max(3);
        let left =
            Layout::vertical([Constraint::Length(desired), Constraint::Min(4)]).split(columns[0]);
        app.list_area = left[0];
        app.history_area = left[1];
        app.details_area = columns[1];
    } else {
        let rows = Layout::vertical([
            Constraint::Length((outer[1].height / 3).max(3)),
            Constraint::Length((outer[1].height / 3).max(3)),
            Constraint::Min(3),
        ])
        .split(outer[1]);
        app.list_area = rows[0];
        app.details_area = rows[1];
        app.history_area = rows[2];
    }
    let selected = app.state.selected();
    let items: Vec<_> = app
        .rows
        .iter()
        .enumerate()
        .map(|(row_index, row)| match row {
            Row::Project(p) => ListItem::new(Line::styled(
                format!(" {}", compact_path(p)),
                Style::default().fg(t.project).add_modifier(Modifier::BOLD),
            )),
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
    let block = pane_block("Sessions", Focus::Sessions, app, t);
    if items.is_empty() {
        f.render_widget(
            Paragraph::new(if app.last_scan.is_none() {
                "Finding sessions…"
            } else if app.filter.is_empty() {
                "No live sessions"
            } else {
                "No matching sessions"
            })
            .block(block),
            app.list_area,
        );
    } else {
        f.render_stateful_widget(
            List::new(items)
                .block(block)
                .highlight_style(
                    Style::default()
                        .bg(t.selected_bg)
                        .fg(t.selected_fg)
                        .add_modifier(Modifier::BOLD),
                )
                .scroll_padding(1),
            app.list_area,
            &mut app.state,
        );
    }
    let block = pane_block("Details", Focus::Details, app, t);
    let lines = details(app, t);
    scroll_pane(f, app.details_area, lines, block, &mut app.detail_scroll);
    let title = if app.expanded {
        "History · Full text"
    } else {
        "History · Previews"
    };
    let block = pane_block(title, Focus::History, app, t);
    let inner = block.inner(app.history_area);
    f.render_widget(block, app.history_area);
    let tabs = Line::from(
        [
            ("1 All", HistoryMode::All),
            ("2 Answers", HistoryMode::Answers),
            ("3 Questions", HistoryMode::Questions),
        ]
        .into_iter()
        .flat_map(|(label, mode)| {
            [
                Span::styled(
                    format!(" {label} "),
                    if app.history_mode == mode {
                        Style::default()
                            .bg(t.selected_bg)
                            .fg(t.selected_fg)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(t.label)
                    },
                ),
                Span::raw(" "),
            ]
        })
        .collect::<Vec<_>>(),
    );
    f.render_widget(Paragraph::new(tabs), Rect { height: 1, ..inner });
    let body_area = Rect {
        y: inner.y + 1,
        height: inner.height.saturating_sub(1),
        ..inner
    };
    let lines = history(app, t);
    scroll_body(f, body_area, lines, &mut app.history_scroll);
    let expansion = if expandable(app) {
        if app.expanded {
            "← previews"
        } else {
            "→ full text"
        }
    } else {
        "All text shown"
    };
    let context = match app.focus {
        Focus::Sessions => "↑↓ select · Enter open",
        Focus::History => "↑↓ scroll · 1/2/3 filter",
        Focus::Details => "↑↓ scroll details",
    };
    let hint = if area.width >= 90 {
        format!(" {context} · Tab pane · {expansion}")
    } else {
        match app.focus {
            Focus::Sessions => " ↑↓ pick · Enter open · Tab pane".into(),
            Focus::History => " ↑↓ scroll · Tab pane · ←/→ text".into(),
            Focus::Details => " ↑↓ scroll · Tab pane".into(),
        }
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!(" {} ", app.focus.label()),
                Style::default()
                    .bg(t.selected_bg)
                    .fg(t.selected_fg)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(hint, Style::default().fg(t.task)),
        ])),
        Rect {
            height: 1,
            ..outer[2]
        },
    );
    let footer = Layout::horizontal([Constraint::Min(0), Constraint::Length(15)]).split(Rect {
        y: outer[2].y + 1,
        height: 1,
        ..outer[2]
    });
    f.render_widget(
        Paragraph::new(if app.notice.is_empty() {
            "t Theme · / Find · r Refresh".into()
        } else {
            safe_text(&app.notice)
        })
        .style(Style::default().fg(
            if app.stale || app.notice.contains("failed") || app.notice.contains("refused") {
                t.error
            } else {
                t.info
            },
        )),
        footer[0],
    );
    f.render_widget(
        Paragraph::new("? Help · q Quit").style(Style::default().fg(t.task)),
        footer[1],
    );
    if app.help {
        f.render_widget(ratatui::widgets::Clear, outer[1]);
        f.render_widget(
            Paragraph::new(
                "Tab / Shift+Tab: focus Sessions → History → Details
Active pane: double border and highlighted title
↑/↓ or j/k: select sessions or scroll focused pane
Wheel: scroll under pointer · click: focus pane
Enter/o: open exact existing iTerm2 pane
→: full history text · ←: message previews · e: toggle
1 All · 2 Agent answers · 3 Your questions · h: next filter
t: next theme (saved and applied immediately)
/: search · r: refresh · q/Ctrl+C: exit
PageUp/PageDown, Home/End: scroll focused pane

● Working · ○ Idle · ! Approval
? Question/action · × Blocked
◆ New response ready · ✓ Pane opened · · Unknown
Opening is not proof of reading. Tracking lasts while Sova is open.

History is a visible terminal snapshot, not a complete archived chat.
Esc/?: close help",
            )
            .block(
                Block::bordered()
                    .title(" Help ")
                    .border_style(Style::default().fg(t.label)),
            )
            .wrap(Wrap { trim: false }),
            outer[1],
        );
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
            "SESSION",
            "TERMINAL",
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
        app.history_scroll = u16::MAX;
        let mut term = ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 25)).unwrap();
        term.draw(|f| draw(f, &mut app, &crate::theme::get("classic").unwrap()))
            .unwrap();
        assert!(app.history_scroll > 0 && app.history_scroll < u16::MAX);
    }
    fn inert_worker() -> Worker {
        let (tx, _) = mpsc::channel();
        let (jumps, _) = mpsc::channel();
        let (_, rx) = mpsc::channel();
        Worker {
            tx,
            jumps,
            rx,
            joins: vec![],
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }
    fn content_app() -> App {
        let mut app = App::new();
        app.reconcile(vec![
            session("a", "/parent/project"),
            session("b", "/parent/project"),
        ]);
        app.content = Some((
            "a".into(),
            Ok(Content {
                state: "Working",
                messages: vec![
                    content::Message {
                        role: Role::You,
                        text: "User question only".into(),
                    },
                    content::Message {
                        role: Role::Action,
                        text: "Tool action only".into(),
                    },
                    content::Message {
                        role: Role::Agent,
                        text: format!(
                            "Agent answer only\n{}END_OF_REPLY",
                            "line of text\n".repeat(30)
                        ),
                    },
                ],
                context: String::new(),
            }),
        ));
        app
    }
    fn text(lines: Vec<Line<'static>>) -> String {
        lines
            .into_iter()
            .map(|l| {
                l.spans
                    .into_iter()
                    .map(|s| s.content.into_owned())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
    #[test]
    fn filters_expand_and_keyboard_focus_have_visible_independent_effects() {
        let mut app = content_app();
        let worker = inert_worker();
        let mut t = crate::theme::get("sunset").unwrap();
        let keypress = |code, app: &mut App, t: &mut Theme| {
            assert!(!key(
                KeyEvent::new(code, KeyModifiers::NONE),
                app,
                &worker,
                t
            ));
        };
        keypress(KeyCode::Tab, &mut app, &mut t);
        assert_eq!(app.focus, Focus::History);
        keypress(KeyCode::Tab, &mut app, &mut t);
        assert_eq!(app.focus, Focus::Details);
        keypress(KeyCode::Down, &mut app, &mut t);
        assert_eq!(app.detail_scroll, 1);
        assert_eq!(app.history_scroll, 0);
        assert_eq!(app.selected().unwrap().id, "a");
        keypress(KeyCode::Char('2'), &mut app, &mut t);
        let answers = text(history(&app, &t));
        assert!(answers.contains("Agent answer only"));
        assert!(!answers.contains("User question only"));
        assert!(!answers.contains("Tool action only"));
        assert!(!answers.contains("END_OF_REPLY"));
        keypress(KeyCode::Right, &mut app, &mut t);
        assert!(app.expanded);
        assert_eq!(app.focus, Focus::History);
        assert!(text(history(&app, &t)).contains("END_OF_REPLY"));
        keypress(KeyCode::Left, &mut app, &mut t);
        assert!(!app.expanded);
        assert!(!text(history(&app, &t)).contains("END_OF_REPLY"));
        keypress(KeyCode::Char('3'), &mut app, &mut t);
        let questions = text(history(&app, &t));
        assert!(questions.contains("User question only"));
        assert!(!questions.contains("Agent answer only"));
        keypress(KeyCode::Right, &mut app, &mut t);
        assert!(!app.expanded);
        assert!(app.notice.contains("nothing to expand"));
        keypress(KeyCode::Char('1'), &mut app, &mut t);
        assert!(text(history(&app, &t)).contains("Tool action only"));
        app.detail_scroll = 7;
        keypress(KeyCode::Down, &mut app, &mut t);
        assert_eq!(app.history_scroll, 1);
        assert_eq!(app.detail_scroll, 7);
    }
    #[test]
    fn responsive_panels_focus_and_mouse_filters_survive_resize() {
        let t = crate::theme::get("sunset").unwrap();
        let mut app = content_app();
        for (w, h) in [(150, 40), (100, 25), (80, 25), (50, 12), (150, 40)] {
            let mut term =
                ratatui::Terminal::new(ratatui::backend::TestBackend::new(w, h)).unwrap();
            term.draw(|f| draw(f, &mut app, &t)).unwrap();
            for a in [app.list_area, app.history_area, app.details_area] {
                assert!(a.width > 2 && a.height >= 3, "{w}×{h}: {a:?}");
                assert!(a.right() <= w && a.bottom() <= h - 2);
            }
            assert!(!app.list_area.intersects(app.history_area));
            assert!(!app.details_area.intersects(app.history_area));
            assert!(!app.list_area.intersects(app.details_area));
            if w >= 90 {
                assert_eq!(app.list_area.x, app.history_area.x);
                assert!(app.details_area.x > app.list_area.x);
            }
            let before = app.row_key();
            mouse(
                event::MouseEvent {
                    kind: MouseEventKind::Down(event::MouseButton::Left),
                    column: app.history_area.x + 10,
                    row: app.history_area.y + 1,
                    modifiers: KeyModifiers::NONE,
                },
                &mut app,
            );
            assert_eq!(app.history_mode, HistoryMode::Answers);
            assert_eq!(app.focus, Focus::History);
            assert_eq!(app.row_key(), before);
            term.draw(|f| draw(f, &mut app, &t)).unwrap();
            assert_eq!(
                term.backend().buffer()[(app.history_area.x, app.history_area.y)].symbol(),
                "╔"
            );
            assert_eq!(
                term.backend().buffer()[(app.list_area.x, app.list_area.y)].symbol(),
                "╭"
            );
            mouse(
                event::MouseEvent {
                    kind: MouseEventKind::ScrollDown,
                    column: app.details_area.x + 2,
                    row: app.details_area.y + 2,
                    modifiers: KeyModifiers::NONE,
                },
                &mut app,
            );
            assert_eq!(app.focus, Focus::Details);
            assert_eq!(app.row_key(), before);
        }
    }
    #[test]
    fn compact_paths_keep_parent_and_home_identity() {
        assert_eq!(
            compact_path_with_home("/Users/alex", Some("/Users/alex")),
            "~/"
        );
        assert_eq!(
            compact_path_with_home("/Users/alex/sova", Some("/Users/alex")),
            "~/sova/"
        );
        assert_eq!(
            compact_path_with_home("/work/cli/sova", Some("/Users/alex")),
            "…/cli/sova/"
        );
        assert_eq!(
            compact_path_with_home("/Users/alexander/project", Some("/Users/alex")),
            "…/alexander/project/"
        );
    }
}
