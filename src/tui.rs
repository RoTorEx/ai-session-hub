use crate::{
    activity::{self, Progress, Snapshot},
    config::Config,
    content::{self, Content},
    model::{self, Session, safe_text},
    theme::Theme,
};
use anyhow::Result;
use crossterm::{
    event::{
        self, DisableFocusChange, DisableMouseCapture, EnableFocusChange, EnableMouseCapture,
        Event, KeyCode, KeyEvent, KeyModifiers, MouseEventKind,
    },
    execute,
};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, List, ListItem, ListState, Paragraph, Scrollbar, ScrollbarOrientation,
        ScrollbarState, Wrap,
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
    time::{Duration, Instant, SystemTime},
};

const HELPER_IDLE_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum StatusFilter {
    #[default]
    All,
    NeedsYou,
    Working,
    Ready,
    Unknown,
}
impl StatusFilter {
    const ALL: [Self; 5] = [
        Self::All,
        Self::NeedsYou,
        Self::Working,
        Self::Ready,
        Self::Unknown,
    ];
    fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::NeedsYou => "Needs you",
            Self::Working => "Working",
            Self::Ready => "Idle",
            Self::Unknown => "Unknown",
        }
    }
    fn includes(self, p: &Progress) -> bool {
        match self {
            Self::All => true,
            Self::NeedsYou => matches!(p.label(), "Approval" | "Question" | "Blocked"),
            Self::Working => p.label() == "Working",
            Self::Ready => matches!(p.label(), "Untracked" | "New reply" | "Opened"),
            Self::Unknown => p.state == activity::State::Unknown,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Focus {
    #[default]
    Sessions,
    History,
    Reader,
}
impl Focus {
    fn label(self) -> &'static str {
        match self {
            Self::Sessions => "Sessions",
            Self::History => "History",
            Self::Reader => "Content",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct HistoryEntry {
    label: &'static str,
    messages: Vec<content::Message>,
}
impl HistoryEntry {
    fn preserves(&self, previous: &Self) -> bool {
        self.label == previous.label
            && self.messages.len() >= previous.messages.len()
            && self
                .messages
                .iter()
                .zip(&previous.messages)
                .all(|(new, old)| new.role == old.role && new.text.starts_with(&old.text))
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
    status_filter: StatusFilter,
    editing: bool,
    focus: Focus,
    history_state: ListState,
    history_area: Rect,
    detail_scroll: u16,
    help: bool,
    technical: bool,
    history_view: bool,
    active: bool,
    sleeping: bool,
    last_user_activity: Instant,
    force_refresh: bool,
    force_content: bool,
    force_activity: bool,
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
            status_filter: StatusFilter::All,
            editing: false,
            focus: Focus::Sessions,
            history_state: ListState::default(),
            history_area: Rect::default(),
            detail_scroll: 0,
            help: false,
            technical: false,
            history_view: false,
            active: true,
            sleeping: false,
            last_user_activity: Instant::now(),
            force_refresh: true,
            force_content: true,
            force_activity: true,
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
    fn set_active(&mut self, active: bool) {
        if self.active == active {
            return;
        }
        self.active = active;
        if active {
            self.force_refresh = true;
            self.force_content = true;
            self.force_activity = true;
            self.last_content = None;
            self.last_activity = None;
            self.dirty = true;
        }
    }
    fn user_activity(&mut self, worker: &Worker) {
        self.last_user_activity = Instant::now();
        if self.sleeping {
            self.sleeping = false;
            worker.warm();
            self.force_refresh = true;
            self.force_content = true;
            self.force_activity = true;
            self.dirty = true;
        }
        self.set_active(true);
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
        self.history_state = ListState::default();
        self.history_view = false;
        self.focus = Focus::Sessions;
        self.selected_since = Instant::now();
        self.last_content = None;
        self.content = None;
        self.dirty = true;
    }
    fn set_content(&mut self, next: Option<(String, Result<Content, String>)>) {
        let previous = selected_entry(self);
        let old = self.history_state.selected().unwrap_or(0);
        self.content = next;
        let entries = history_entries(self);
        let matched = previous.as_ref().and_then(|p| {
            entries
                .iter()
                .enumerate()
                .filter(|(_, m)| m.preserves(p))
                .min_by_key(|(i, _)| i.abs_diff(old))
                .map(|(i, _)| i)
        });
        let index = matched.or_else(|| {
            if entries.is_empty() {
                None
            } else if previous.is_none() {
                Some(0)
            } else {
                Some(old.min(entries.len() - 1))
            }
        });
        self.history_state.select(index);
        if previous.is_some() && matched.is_none() {
            if self.focus == Focus::Reader {
                self.focus = Focus::History;
            }
            self.detail_scroll = 0;
        }
    }
    fn move_history(&mut self, delta: isize) {
        let len = history_entries(self).len();
        if len == 0 {
            self.history_state.select(None);
            return;
        }
        let old = self.history_state.selected();
        let next = old.unwrap_or(0).saturating_add_signed(delta).min(len - 1);
        self.history_state.select(Some(next));
        if old != Some(next) {
            self.detail_scroll = 0;
        }
        // An open reader follows the explicitly selected iteration.
    }
    fn rebuild(&mut self, preferred: Option<String>) {
        let old = self.state.selected().unwrap_or(0);
        let needle = self.filter.to_lowercase();
        let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (i, s) in self.sessions.iter().enumerate() {
            if !self
                .status_filter
                .includes(&self.activity.get(&s.id).cloned().unwrap_or_default())
            {
                continue;
            }
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
            if std::env::var("HOME").ok().as_deref() != Some(project.as_str()) {
                self.rows.push(Row::Project(project));
            }
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
        for (id, metadata) in snapshot.metadata {
            if let Some(p) = self.activity.get_mut(&id) {
                p.native_metadata = true;
                p.model = metadata.model.or(p.model.take());
                p.reasoning = metadata.reasoning.or(p.reasoning.take());
                p.updated_at = metadata.updated_at;
                p.requested_at = metadata.requested_at;
                p.request = metadata.request.or(p.request.take());
                p.last_message = metadata.last_message;
            }
        }
        if !self.history_view
            && let Some(id) = self.row_key()
        {
            let old = before.get(&id);
            let new = self.activity.get(&id);
            if old.map(|p| (&p.request, &p.last_message))
                != new.map(|p| (&p.request, &p.last_message))
            {
                self.last_content = None;
            }
        }
        self.dirty |= self.activity != before;
        if self.activity != before && self.status_filter != StatusFilter::All {
            let key = self.row_key();
            self.rebuild(key.clone());
            if self.row_key() != key {
                self.selection_changed();
            }
        }
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
    Warm,
    Sleep,
    Refresh(bool),
    Content(Box<Session>),
    Activity(Vec<Session>),
    Stop,
}
enum JumpRequest {
    Warm,
    Sleep,
    Jump(Box<Session>),
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
    jumps: mpsc::Sender<JumpRequest>,
    rx: mpsc::Receiver<Response>,
    joins: Vec<thread::JoinHandle<()>>,
    cancelled: Arc<AtomicBool>,
}
impl Worker {
    fn start(metadata_seconds: u64) -> Self {
        let (tx, requests) = mpsc::channel();
        let (results, rx) = mpsc::channel();
        let (jumps, jump_requests) = mpsc::channel::<JumpRequest>();
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
                    Request::Warm => {
                        let _ = collector.terminal.warm();
                        continue;
                    }
                    Request::Sleep => {
                        collector.terminal.sleep();
                        continue;
                    }
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
            while let Ok(request) = jump_requests.recv() {
                if jump_stopping.load(Ordering::Relaxed) {
                    break;
                }
                let s = match request {
                    JumpRequest::Warm => {
                        let _ = terminal.warm();
                        continue;
                    }
                    JumpRequest::Sleep => {
                        terminal.sleep();
                        continue;
                    }
                    JumpRequest::Jump(s) => s,
                    JumpRequest::Stop => break,
                };
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
    fn sleep(&self) {
        let _ = self.tx.send(Request::Sleep);
        let _ = self.jumps.send(JumpRequest::Sleep);
    }
    fn warm(&self) {
        let _ = self.tx.send(Request::Warm);
        let _ = self.jumps.send(JumpRequest::Warm);
    }
    fn refresh(&self, app: &mut App, force: bool) -> bool {
        if !app.busy && self.tx.send(Request::Refresh(force)).is_ok() {
            app.busy = true;
            true
        } else {
            false
        }
    }
    fn jump(&self, app: &mut App) {
        if app.jump_pending {
            return;
        }
        if let Some(s) = app.selected().cloned()
            && self.jumps.send(JumpRequest::Jump(Box::new(s))).is_ok()
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
        let _ = self.jumps.send(JumpRequest::Stop);
        for h in self.joins.drain(..) {
            let _ = h.join();
        }
    }
}
fn poll_background(app: &mut App, worker: &Worker, config: &Config) {
    if !app.sleeping && app.last_user_activity.elapsed() >= HELPER_IDLE_TIMEOUT {
        app.sleeping = true;
        app.dirty = true;
        worker.sleep();
    }
    if !app.active || app.sleeping {
        return;
    }
    if (app.force_refresh
        || app
            .last_scan
            .is_some_and(|t| t.elapsed() >= Duration::from_secs(config.refresh_seconds)))
        && worker.refresh(app, app.force_refresh)
    {
        app.force_refresh = false;
    }
    // Details gets one full visible-screen read per selection; History keeps refreshing.
    if (app.history_view || app.last_content.is_none())
        && !app.content_pending
        && app.selected_since.elapsed() >= Duration::from_millis(150)
        && (app.force_content
            || app
                .last_content
                .is_none_or(|t| t.elapsed() >= Duration::from_secs(config.content_seconds)))
        && let Some(s) = app.selected().cloned()
        && worker.tx.send(Request::Content(Box::new(s))).is_ok()
    {
        app.content_pending = true;
        app.force_content = false;
    }
    if !app.activity_pending
        && !app.sessions.is_empty()
        && (app.force_activity
            || app
                .last_activity
                .is_none_or(|t| t.elapsed() >= Duration::from_secs(config.activity_seconds)))
        && worker
            .tx
            .send(Request::Activity(app.sessions.clone()))
            .is_ok()
    {
        app.activity_pending = true;
        app.force_activity = false;
    }
}
struct TerminalGuard;
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = execute!(std::io::stdout(), DisableMouseCapture, DisableFocusChange);
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
        execute!(std::io::stdout(), EnableMouseCapture, EnableFocusChange)?;
        let mut app = App::new();
        let mut last_clock_draw = Instant::now();
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
                                app.set_content(next);
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
                                app.observe(Snapshot {
                                    sessions: app
                                        .sessions
                                        .iter()
                                        .map(|s| (s.id.clone(), None))
                                        .collect(),
                                    focused: None,
                                    metadata: Default::default(),
                                });
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
                            Ok(()) => "Session opened · AI Session Hub stays here".into(),
                            Err(e) => format!("Jump refused: {e}"),
                        };
                        app.dirty = true;
                    }
                }
            }
            poll_background(&mut app, &worker, &config);
            if app.active
                && !app.sleeping
                && last_clock_draw.elapsed() >= Duration::from_secs(1)
                && app.activity.values().any(|p| {
                    [p.updated_at, p.requested_at]
                        .into_iter()
                        .flatten()
                        .any(|time| {
                            time.elapsed()
                                .is_ok_and(|age| age < Duration::from_secs(3600))
                        })
                })
            {
                app.dirty = true;
            }
            if app.dirty {
                last_clock_draw = Instant::now();
                terminal.draw(|f| draw(f, &mut app, &theme))?;
                app.dirty = false;
            }
            if event::poll(Duration::from_millis(if app.active && !app.sleeping {
                100
            } else {
                1000
            }))? {
                match event::read()? {
                    Event::Key(k) if k.kind == event::KeyEventKind::Press => {
                        app.dirty = true;
                        if key(k, &mut app, &worker, &mut theme) {
                            break;
                        }
                        app.user_activity(&worker);
                    }
                    Event::FocusLost => app.set_active(false),
                    Event::FocusGained => app.user_activity(&worker),
                    Event::Resize(_, _) => app.dirty = true,
                    Event::Mouse(m) => {
                        app.user_activity(&worker);
                        mouse(m, &mut app);
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
fn mouse(m: event::MouseEvent, app: &mut App) {
    if app.help || app.editing {
        return;
    }
    if matches!(
        m.kind,
        MouseEventKind::ScrollDown | MouseEventKind::ScrollUp
    ) {
        let delta = if m.kind == MouseEventKind::ScrollDown {
            1
        } else {
            -1
        };
        if app.history_area.contains((m.column, m.row).into()) {
            app.move_history(delta);
        } else if app.details_area.contains((m.column, m.row).into()) {
            app.detail_scroll = app.detail_scroll.saturating_add_signed(delta as i16 * 3);
        } else if app.list_area.contains((m.column, m.row).into()) {
            app.move_selection(delta);
        } else {
            return;
        }
        app.dirty = true;
    }
}
fn navigate(app: &mut App, delta: isize) {
    match app.focus {
        Focus::Sessions => app.move_selection(delta),
        Focus::History => app.move_history(delta),
        Focus::Reader => {
            app.detail_scroll = app
                .detail_scroll
                .saturating_add_signed(delta.clamp(i16::MIN as isize, i16::MAX as isize) as i16)
        }
    }
}
fn selected_content(app: &App) -> Option<&Result<Content, String>> {
    let id = &app.selected()?.id;
    app.content
        .as_ref()
        .filter(|(key, _)| key == id)
        .map(|(_, c)| c)
}
fn history_entries(app: &App) -> Vec<HistoryEntry> {
    match selected_content(app) {
        Some(Ok(c)) if !c.messages.is_empty() => {
            let mut entries: Vec<HistoryEntry> = Vec::new();
            for message in &c.messages {
                if message.role == content::Role::You || entries.is_empty() {
                    entries.push(HistoryEntry {
                        label: if message.role == content::Role::You {
                            "Iteration"
                        } else {
                            "Partial iteration"
                        },
                        messages: Vec::new(),
                    });
                }
                entries.last_mut().unwrap().messages.push(message.clone());
            }
            entries
        }
        Some(Ok(c)) if !c.context.is_empty() => vec![HistoryEntry {
            label: "Terminal snapshot",
            messages: vec![content::Message {
                role: content::Role::Agent,
                text: c.context.clone(),
            }],
        }],
        _ => vec![],
    }
}
fn selected_entry(app: &App) -> Option<HistoryEntry> {
    let entries = history_entries(app);
    entries
        .get(app.history_state.selected().unwrap_or(0))
        .cloned()
}
fn switch_view(app: &mut App, history_view: bool) {
    if history_view && !app.history_view {
        app.force_content = true;
        app.last_content = None;
    }
    app.history_view = history_view;
    app.focus = if history_view {
        Focus::History
    } else {
        Focus::Sessions
    };
    app.detail_scroll = 0;
    app.notice.clear();
}
fn toggle_reader(app: &mut App) {
    if app.history_view {
        app.focus = if app.focus == Focus::Reader {
            Focus::History
        } else {
            Focus::Reader
        };
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
        KeyCode::Char('q' | 'й') => return true,
        KeyCode::Esc if app.history_view => switch_view(app, false),
        KeyCode::Esc if app.technical => {
            app.technical = false;
            app.detail_scroll = 0;
        }
        KeyCode::Esc => return true,
        KeyCode::Char('/') => {
            switch_view(app, false);
            app.editing = true;
        }
        KeyCode::Char('?') => app.help = true,
        KeyCode::Char('i' | 'ш') if !app.history_view => {
            app.technical = !app.technical;
            app.detail_scroll = 0;
        }
        KeyCode::Char('f' | 'F' | 'а' | 'А') => switch_view(app, !app.history_view),
        KeyCode::Tab | KeyCode::BackTab if !app.history_view => {
            let index = StatusFilter::ALL
                .iter()
                .position(|v| *v == app.status_filter)
                .unwrap_or(0);
            let offset = if k.code == KeyCode::BackTab { 4 } else { 1 };
            app.status_filter = StatusFilter::ALL[(index + offset) % 5];
            let old = app.row_key();
            app.rebuild(old.clone());
            if app.row_key() != old {
                app.selection_changed();
            }
        }
        KeyCode::Tab | KeyCode::BackTab => toggle_reader(app),
        KeyCode::Right if app.focus != Focus::Reader => navigate(app, 1),
        KeyCode::Left if app.focus != Focus::Reader => navigate(app, -1),
        KeyCode::Char('e' | 'у') => toggle_reader(app),
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
        KeyCode::Char('o' | 'щ') => worker.jump(app),
        KeyCode::Enter if app.focus == Focus::History => toggle_reader(app),
        KeyCode::Enter if app.focus == Focus::Sessions => worker.jump(app),
        KeyCode::Up | KeyCode::Char('k' | 'л') => navigate(app, -1),
        KeyCode::Down | KeyCode::Char('j' | 'о') => navigate(app, 1),
        KeyCode::PageUp => navigate(app, -10),
        KeyCode::PageDown => navigate(app, 10),
        KeyCode::Home => match app.focus {
            Focus::Sessions => app.move_selection(isize::MIN),
            Focus::History => app.move_history(isize::MIN),
            Focus::Reader => app.detail_scroll = 0,
        },
        KeyCode::End => match app.focus {
            Focus::Sessions => app.move_selection(isize::MAX),
            Focus::History => app.move_history(isize::MAX),
            Focus::Reader => app.detail_scroll = u16::MAX,
        },
        _ => {}
    }
    false
}
fn field(label: &str, value: String, t: &Theme) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label}  "), Style::default().fg(t.label)),
        Span::styled(safe_text(&value), Style::default().fg(t.task)),
    ])
}
fn preview(text: &str) -> String {
    let prefix: String = safe_text(text).chars().take(100).collect();
    let prefix = prefix.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() > 100 {
        format!("{prefix}…")
    } else {
        prefix
    }
}
fn wrap_preview(text: &str, width: u16) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let mut line = String::new();
    let mut cells = 0;
    for c in text.chars() {
        let next = Span::raw(c.to_string()).width();
        if cells + next > width.max(1) as usize && !line.is_empty() {
            lines.push(Line::raw(std::mem::take(&mut line)));
            cells = 0;
        }
        line.push(c);
        cells += next;
    }
    lines.push(Line::raw(line));
    lines
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
        Style::default().fg(t.label),
    )
}
// Preview complete wrapped lines so long content cannot bury the next section.
fn detail_preview(
    lines: &mut Vec<Line<'static>>,
    value: &str,
    width: u16,
    limit: usize,
    t: &Theme,
) {
    let mut wrapped = Vec::new();
    let mut current = String::new();
    for word in safe_text(value).split_whitespace() {
        let next = if current.is_empty() {
            word.to_owned()
        } else {
            format!("{current} {word}")
        };
        if Span::raw(&next).width() <= width as usize {
            current = next;
            continue;
        }
        if !current.is_empty() {
            wrapped.push(Line::raw(std::mem::take(&mut current)));
        }
        let mut parts = wrap_preview(word, width);
        current = parts.pop().map(|l| l.to_string()).unwrap_or_default();
        wrapped.extend(parts);
    }
    if !current.is_empty() {
        wrapped.push(Line::raw(current));
    }
    let count = wrapped.len();
    for (i, line) in wrapped.into_iter().take(limit).enumerate() {
        let text = line.to_string();
        let text = if i + 1 == limit && count > limit {
            fit_cell(&format!("{text}…"), width as usize)
                .trim_end()
                .to_owned()
        } else {
            text
        };
        lines.push(Line::styled(text, Style::default().fg(t.task)));
    }
}
fn details(app: &App, t: &Theme) -> Vec<Line<'static>> {
    let Some(s) = app.selected() else {
        return vec![Line::raw("Select a session to see its details")];
    };
    let width = app.details_area.width.saturating_sub(2);
    let preview_lines = if app.details_area.height >= 28 { 3 } else { 2 };
    let progress = app.activity.get(&s.id).cloned().unwrap_or_default();
    let mut lines = vec![
        Line::styled(
            session_title(s),
            Style::default().fg(t.project).add_modifier(Modifier::BOLD),
        ),
        Line::raw(format!(
            "{} {} · {}",
            progress.symbol(),
            progress.label(),
            s.provider
        )),
    ];
    if app.technical {
        lines.push(Line::default());
        lines.push(separator("DATA SOURCES", width, t));
        body(
            &mut lines,
            if progress.native_metadata {
                "Metadata: linked provider session. Missing fields may come from visible terminal text."
            } else {
                "Provider metadata is unavailable: no uniquely bound session file. Model, reasoning and event times may be absent. Visible terminal text is used when available."
            },
            t,
        );
        body(
            &mut lines,
            "Times use Europe/Warsaw (CET/CEST). Terminal change times are observations; provider times are conversation events. Opening a pane does not confirm reading.",
            t,
        );
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
            lines.push(field("TTY", p.tty.clone(), t));
            lines.push(field("Pane UUID", p.id.clone(), t));
        } else {
            lines.push(field(
                "Jump unavailable",
                s.link_reason.clone().unwrap_or_default(),
                t,
            ));
        }
        lines.push(Line::default());
        lines.push(separator("PROCESS", width, t));
        lines.push(field("PID", s.pid.to_string(), t));
        lines.push(field("Process ID", s.id.clone(), t));
        lines.push(field(
            "Folder",
            s.cwd.clone().unwrap_or_else(|| "Unknown".into()),
            t,
        ));
        if s.cwd.as_deref() != Some(&s.project) {
            lines.push(field("Git root", s.project.clone(), t));
        }
        return lines;
    }
    if progress.label() == "Untracked" {
        lines.push(Line::styled(
            progress.explanation(),
            Style::default().fg(t.label),
        ));
    } else if matches!(progress.label(), "Question" | "Blocked") {
        lines.push(Line::styled(
            progress.opening(),
            Style::default().fg(t.label),
        ));
    }
    let content = selected_content(app).and_then(|r| r.as_ref().ok());
    let model = progress
        .model
        .as_deref()
        .or_else(|| content.and_then(|c| c.model.as_deref()));
    let reasoning = progress
        .reasoning
        .as_deref()
        .or_else(|| content.and_then(|c| c.reasoning.as_deref()));
    if let Some(model) = model {
        lines.push(field("Model", model.into(), t));
    }
    if let Some(reasoning) = reasoning {
        lines.push(field("Reasoning", reasoning.into(), t));
    }
    if model.is_none() && reasoning.is_none() {
        lines.push(Line::styled(
            "Model/reasoning not exposed",
            Style::default().fg(t.label),
        ));
    }
    lines.push(field(
        "Updated",
        if progress.updated_at.is_some() {
            progress_time(&progress, false, SystemTime::now())
        } else {
            "Time unavailable".into()
        },
        t,
    ));
    if progress.requested_at.is_some() {
        lines.push(field(
            "Requested",
            progress_time(&progress, true, SystemTime::now()),
            t,
        ));
    }
    lines.push(separator("PROJECT", width, t));
    body(&mut lines, s.cwd.as_deref().unwrap_or(&s.project), t);
    let visible_request = content.and_then(|c| {
        c.messages
            .iter()
            .rev()
            .find(|m| m.role == content::Role::You)
            .map(|m| m.text.as_str())
    });
    let request = progress.request.as_deref().or(visible_request);
    let answer = progress.last_message.as_deref().or_else(|| {
        if progress.native_metadata
            || (progress.request.is_some() && progress.request.as_deref() != visible_request)
        {
            return None;
        }
        content.and_then(|c| {
            c.messages
                .iter()
                .rev()
                .take_while(|m| m.role != content::Role::You)
                .find(|m| m.role == content::Role::Agent)
                .map(|m| m.text.as_str())
        })
    });
    lines.push(Line::default());
    lines.push(separator("LATEST REQUEST", width, t));
    if let Some(request) = request {
        detail_preview(&mut lines, request, width, preview_lines, t);
    } else {
        let unavailable = match selected_content(app) {
            Some(Err(error)) => format!("Session content unavailable: {error}"),
            _ if app.content_pending || app.last_content.is_none() => {
                "Reading selected session…".into()
            }
            _ => "Request is outside the visible terminal screen.".into(),
        };
        body(&mut lines, &unavailable, t);
    }
    lines.push(Line::default());
    lines.push(separator("LATEST ANSWER", width, t));
    if let Some(answer) = answer {
        detail_preview(&mut lines, answer, width, preview_lines, t);
    } else {
        body(
            &mut lines,
            if progress.state == activity::State::Working {
                "Agent is working."
            } else {
                "No answer in the available session content."
            },
            t,
        );
    }
    lines
}

fn progress_time(progress: &Progress, request: bool, now: SystemTime) -> String {
    let time = if request {
        progress.requested_at
    } else {
        progress.updated_at
    };
    event_time(time, now)
}

fn event_time(value: Option<SystemTime>, now: SystemTime) -> String {
    let Some(value) = value else {
        return "—".into();
    };
    let age = now.duration_since(value).unwrap_or_default().as_secs();
    if age < 60 {
        return format!("{age}s");
    }
    if age < 3600 {
        return format!("{}m", age / 60);
    }
    let date: chrono::DateTime<chrono::Utc> = value.into();
    date.with_timezone(&chrono_tz::Europe::Warsaw)
        .format("%d %b %H:%M %Z")
        .to_string()
}

fn fit_cell(text: &str, width: usize) -> String {
    let mut result = String::new();
    let mut cells = 0;
    let clean = safe_text(text);
    let truncated = Span::raw(&clean).width() > width;
    let limit = width.saturating_sub(usize::from(truncated));
    for c in clean.chars() {
        let w = Span::raw(c.to_string()).width();
        if cells + w > limit {
            break;
        }
        result.push(c);
        cells += w;
    }
    if truncated && width > 0 {
        result.push('…');
        cells += 1;
    }
    result.push_str(&" ".repeat(width.saturating_sub(cells)));
    result
}

// Always retain Updated; omit the redundant request-time column in the list.
fn session_columns(width: u16) -> (usize, usize) {
    (9, if width >= 85 { 19 } else { 7 })
}
fn table_time(progress: &Progress, now: SystemTime, width: usize) -> String {
    let full = progress_time(progress, false, now);
    if Span::raw(&full).width() <= width {
        return full;
    }
    let Some(value) = progress.updated_at else {
        return full;
    };
    let date: chrono::DateTime<chrono::Utc> = value.into();
    let now: chrono::DateTime<chrono::Utc> = now.into();
    let local = date.with_timezone(&chrono_tz::Europe::Warsaw);
    if local.date_naive() == now.with_timezone(&chrono_tz::Europe::Warsaw).date_naive() {
        local.format("%H:%M").to_string()
    } else {
        local.format("%d %b").to_string()
    }
}
fn compact_path_with_home(path: &str, home: Option<&str>) -> String {
    if home == Some(path) {
        return "~".into();
    }
    std::path::Path::new(path)
        .file_name()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.into())
}
fn project_label(app: &App, path: &str) -> String {
    let home = std::env::var("HOME").ok();
    let name = compact_path_with_home(path, home.as_deref());
    if app
        .rows
        .iter()
        .filter(|r| matches!(r,Row::Project(p) if compact_path_with_home(p,home.as_deref())==name))
        .count()
        > 1
    {
        let p = std::path::Path::new(path);
        let parent = p
            .parent()
            .and_then(|p| p.file_name())
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        format!("{parent}/{name}")
    } else {
        name
    }
}
fn reader(app: &App, t: &Theme) -> Vec<Line<'static>> {
    let Some(entry) = selected_entry(app) else {
        let message = match selected_content(app) {
            Some(Err(error)) => format!("Content unavailable: {error}"),
            Some(Ok(_)) => "No iteration in the visible terminal snapshot".into(),
            None if app.selected().is_some() => "Reading selected terminal…".into(),
            None => "No session selected · f returns to Sessions".into(),
        };
        return vec![Line::raw(message)];
    };
    let mut lines = Vec::new();
    for message in &entry.messages {
        lines.push(separator(
            message.role.label(),
            app.details_area.width.saturating_sub(2),
            t,
        ));
        body(&mut lines, &message.text, t);
        lines.push(Line::default());
    }
    lines
}
fn draw_history(f: &mut Frame, app: &mut App, t: &Theme) {
    let entries = history_entries(app);
    let block = pane_block("History · oldest → newest", Focus::History, app, t);
    if entries.is_empty() {
        let message = if app.selected().is_none() {
            "Select a session to see its history".into()
        } else {
            match selected_content(app) {
                Some(Err(e)) => format!("Content unavailable: {e}"),
                Some(Ok(_)) => "No messages in the visible terminal snapshot".into(),
                None => "Reading selected terminal…".into(),
            }
        };
        app.history_state.select(None);
        f.render_widget(
            Paragraph::new(message)
                .wrap(Wrap { trim: false })
                .block(block),
            app.history_area,
        );
        return;
    }
    let selected = app
        .history_state
        .selected()
        .unwrap_or(0)
        .min(entries.len() - 1);
    app.history_state.select(Some(selected));
    let items: Vec<_> = entries
        .iter()
        .map(|m| {
            let mut lines = vec![separator(
                m.label,
                app.history_area.width.saturating_sub(4),
                t,
            )];
            for message in &m.messages {
                lines.push(Line::styled(
                    message.role.label(),
                    Style::default().fg(t.label),
                ));
                lines.extend(wrap_preview(
                    &preview(&message.text),
                    app.history_area.width.saturating_sub(4),
                ));
            }
            lines.push(Line::default());
            ListItem::new(lines)
        })
        .collect();
    f.render_stateful_widget(
        List::new(items)
            .block(block)
            .style(Style::default().fg(t.task))
            .highlight_symbol("› ")
            .highlight_style(Style::default().bg(t.selected_bg).fg(t.selected_fg)),
        app.history_area,
        &mut app.history_state,
    );
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
    Block::default()
        .title(format!(" {title} "))
        .borders(if focus == Focus::Reader {
            Borders::LEFT
        } else {
            Borders::TOP
        })
        .border_style(Style::default().fg(if app.focus == focus {
            if focus == Focus::Reader {
                t.detail_border
            } else {
                t.list_border
            }
        } else {
            t.status
        }))
        .title_style(Style::default().fg(t.task))
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
                "AI Session Hub v{} · resize to 50 × 12 · q exit",
                env!("CARGO_PKG_VERSION")
            )),
            area,
        );
        return;
    }
    let outer = Layout::vertical([
        Constraint::Length(3),
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
    let mut heading = vec![Line::from(vec![
        Span::styled(
            format!("AI Session Hub v{}", env!("CARGO_PKG_VERSION")),
            Style::default().fg(t.task).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(
                " · {} · {} sessions{}{}{filter}",
                t.name,
                app.sessions.len(),
                if app.stale { " · STALE" } else { "" },
                if app.sleeping { " · SLEEP" } else { "" }
            ),
            Style::default().fg(t.status),
        ),
    ])];
    if !app.history_view {
        heading.push(Line::from(
            StatusFilter::ALL
                .iter()
                .map(|filter| {
                    let count = app
                        .sessions
                        .iter()
                        .filter(|s| {
                            filter.includes(&app.activity.get(&s.id).cloned().unwrap_or_default())
                        })
                        .count();
                    Span::styled(
                        format!(" {} {count} ", filter.label()),
                        if *filter == app.status_filter {
                            Style::default().fg(t.selected_fg).bg(t.selected_bg)
                        } else {
                            Style::default().fg(t.status)
                        },
                    )
                })
                .collect::<Vec<_>>(),
        ));
        heading.push(Line::styled(
            "Tab / Shift+Tab filter",
            Style::default().fg(t.label),
        ));
    } else {
        if let Some(session) = app.selected() {
            let progress = app.activity.get(&session.id).cloned().unwrap_or_default();
            heading.push(Line::raw(format!(
                "{} · {} · {} {} · o Open",
                session_title(session),
                session.provider,
                progress.symbol(),
                progress.label()
            )));
            heading.push(Line::raw(format!(
                "Folder: {}",
                safe_text(session.cwd.as_deref().unwrap_or("Unknown"))
            )));
        } else {
            heading.push(Line::raw("No session selected · f Sessions"));
        }
    }
    f.render_widget(
        Paragraph::new(heading).style(Style::default().fg(t.task)),
        outer[0],
    );
    let left = if app.history_view { 40 } else { 65 };
    let columns = Layout::horizontal([
        Constraint::Percentage(left),
        Constraint::Percentage(100 - left),
    ])
    .split(outer[1]);
    app.details_area = columns[1];
    app.list_area = if app.history_view {
        Rect::default()
    } else {
        columns[0]
    };
    app.history_area = if app.history_view {
        columns[0]
    } else {
        Rect::default()
    };
    if !app.history_view {
        let selected = app.state.selected();
        let row_width = app.list_area.width.saturating_sub(2);
        let (status_w, update_w) = session_columns(row_width);
        let name_w = (row_width as usize).saturating_sub(status_w + update_w + 2);
        let now = SystemTime::now();
        let items: Vec<_> = app
            .rows
            .iter()
            .enumerate()
            .map(|(row_index, row)| match row {
                Row::Project(p) => ListItem::new(Line::styled(
                    format!(" {}", project_label(app, p)),
                    Style::default().fg(t.project),
                )),
                Row::Session(i) => {
                    let s = &app.sessions[*i];
                    let active = selected == Some(row_index);
                    let progress = app.activity.get(&s.id).cloned().unwrap_or_default();
                    let color = match progress.label() {
                        "Approval" | "Blocked" | "Question" => t.error,
                        "Working" => t.info,
                        "New reply" => t.project,
                        _ => t.status,
                    };
                    let name = format!("{} {}", progress.symbol(), session_title(s));
                    let normal = Style::default().fg(if active { t.selected_fg } else { t.task });
                    ListItem::new(Line::from(vec![
                        Span::styled(fit_cell(&name, name_w), normal),
                        Span::raw(" "),
                        Span::styled(
                            fit_cell(progress.label(), status_w),
                            Style::default().fg(if active { t.selected_fg } else { color }),
                        ),
                        Span::raw(" "),
                        Span::styled(
                            fit_cell(&table_time(&progress, now, update_w), update_w),
                            normal,
                        ),
                    ]))
                }
            })
            .collect();
        let block = pane_block("Sessions", Focus::Sessions, app, t);
        let table_area = Rect {
            y: app.list_area.y + 2,
            height: app.list_area.height.saturating_sub(2),
            ..app.list_area
        };
        let header = format!(
            "  {} {} {}",
            fit_cell("Tasks", name_w),
            fit_cell("Status", status_w),
            fit_cell("Updated", update_w)
        );
        f.render_widget(block.clone(), app.list_area);
        f.render_widget(
            Paragraph::new(header).style(Style::default().fg(t.label)),
            Rect {
                y: app.list_area.y + 1,
                height: 1,
                ..app.list_area
            },
        );
        if items.is_empty() {
            f.render_widget(
                Paragraph::new(if app.last_scan.is_none() {
                    "Finding sessions…"
                } else if app.filter.is_empty() {
                    "No live sessions"
                } else {
                    "No matching sessions"
                }),
                table_area,
            );
        } else {
            f.render_stateful_widget(
                List::new(items)
                    .highlight_style(Style::default().bg(t.selected_bg).fg(t.selected_fg))
                    .highlight_symbol("› ")
                    .scroll_padding(1),
                table_area,
                &mut app.state,
            );
        }
    }
    let block = pane_block(
        if app.history_view {
            "Content · Tab scroll"
        } else {
            if app.technical {
                "Session Info"
            } else {
                "Details"
            }
        },
        Focus::Reader,
        app,
        t,
    );
    let lines = if app.history_view {
        reader(app, t)
    } else {
        details(app, t)
    };
    scroll_pane(f, app.details_area, lines, block, &mut app.detail_scroll);
    if app.history_view {
        draw_history(f, app, t);
    }
    let context = match app.focus {
        Focus::Sessions if area.width < 90 => "↑↓ · Enter open · f history · i info",
        Focus::Sessions => "↑↓ select · Enter open · f history · i info",
        Focus::History => "↑↓ iteration · Tab scroll · o open",
        Focus::Reader => "↑↓ scroll · Tab history · o open",
    };
    f.render_widget(
        Paragraph::new(format!("{} · {context}", app.focus.label()))
            .style(Style::default().fg(t.task)),
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
                "f: switch Sessions | Details ↔ History | Content
Startup: Sessions. Both panels change together; selected session is retained.
Polling pauses without terminal focus; returning refreshes immediately.
After 60 seconds without input, helpers exit; input/focus return wakes them.
History header keeps session name, provider/status and folder visible.
Tab / Shift+Tab: status filter in Sessions; History ↔ Content in history
↑/↓ or j/k: select a session/iteration or scroll Content
←/→: previous/next session or history iteration
e / Enter in History: focus Content; e returns to History
Esc in History/Content: return to Sessions | Details
History: oldest at top, newest at bottom; start at the top
Each iteration groups You, Action and Agent; previews of 100 characters
Wheel: navigate or scroll under the pointer without changing focus
Enter in Sessions / o: open exact existing iTerm2 pane
t: next theme (saved and applied immediately)
/: search · r: refresh · q/Ctrl+C: exit
PageUp/PageDown, Home/End: navigate the focused pane

● Working · ○ Untracked opening · ! Approval
? Question/action · × Blocked
◆ New reply (not opened) · ✓ Opened · · Unknown
Untracked: idle; opening history unknown. Idle filter includes Untracked, New reply and Opened.
i: show session information and data sources; Esc returns to Details.
Opening is not proof of reading. Tracking lasts while AI Session Hub is open.

History is a visible terminal snapshot, not a complete archived chat.
Message timestamps are unavailable; none are inferred.
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
    #[test]
    fn timestamps_switch_at_one_hour_and_follow_warsaw_dst() {
        let time =
            |v: &str| -> SystemTime { chrono::DateTime::parse_from_rfc3339(v).unwrap().into() };
        let summer = time("2026-10-20T09:00:00Z");
        assert_eq!(
            event_time(Some(summer), summer + Duration::from_secs(59)),
            "59s"
        );
        assert_eq!(
            event_time(Some(summer), summer + Duration::from_secs(3599)),
            "59m"
        );
        assert_eq!(
            event_time(Some(summer), summer + Duration::from_secs(3600)),
            "20 Oct 11:00 CEST"
        );
        let winter = time("2026-12-20T23:30:00Z");
        assert_eq!(
            event_time(Some(winter), winter + Duration::from_secs(7200)),
            "21 Dec 00:30 CET"
        );
        let mut compact = Progress::default();
        compact.updated_at = Some(summer);
        assert_eq!(
            table_time(&compact, summer + Duration::from_secs(3600), 7),
            "11:00"
        );
        assert_eq!(
            table_time(&compact, summer + Duration::from_secs(86_400), 7),
            "20 Oct"
        );
        assert_eq!(
            table_time(&compact, summer + Duration::from_secs(3600), 19),
            "20 Oct 11:00 CEST"
        );
        assert_eq!(event_time(None, summer), "—");
        assert_eq!(fit_cell("界abcdef", 5), "界ab…");
    }

    #[test]
    fn status_filters_keep_matching_selection_and_reconcile_on_activity_changes() {
        let mut app = App::new();
        app.reconcile(vec![session("a", "/alpha"), session("b", "/alpha")]);
        app.activity.entry("a".into()).or_default().state = activity::State::Working;
        app.status_filter = StatusFilter::Working;
        app.rebuild(Some("a".into()));
        assert_eq!(app.row_key().as_deref(), Some("a"));
        app.observe(Snapshot {
            sessions: vec![(
                "a".into(),
                Some(activity::Observation::parse(
                    "› Task\n• Done\n› draft\n70% context left",
                    crate::process::Provider::Codex,
                )),
            )],
            focused: None,
            metadata: Default::default(),
        });
        assert!(app.row_key().is_none());
        app.status_filter = StatusFilter::All;
        app.rebuild(None);
        assert!(app.row_key().is_some());
    }

    #[test]
    fn narrow_rows_keep_update_and_separate_unicode_titles_from_status() {
        let mut app = content_app();
        app.sessions[0].location = Some(crate::iterm::Pane {
            id: "pane".into(),
            tty: "/dev/ttys001".into(),
            root_pid: 1,
            window_id: 1,
            window: "Agents".into(),
            tab: "Work".into(),
            tab_index: 0,
            pane_index: 0,
            name: "界界 Очень длинное название задачи с кириллицей и продолжением".repeat(3),
        });
        let mut progress = Progress::default();
        progress.state = activity::State::Idle;
        progress.updated_at = Some(SystemTime::now() - Duration::from_secs(15));
        app.activity.insert("a".into(), progress);
        let t = crate::theme::get("sunset").unwrap();
        for width in [50, 80, 100, 120, 160] {
            let mut term =
                ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, 30)).unwrap();
            term.draw(|f| draw(f, &mut app, &t)).unwrap();
            let rows = (app.list_area.y..app.list_area.bottom())
                .map(|y| {
                    (0..app.list_area.right())
                        .map(|x| term.backend().buffer()[(x, y)].symbol())
                        .collect::<String>()
                })
                .collect::<Vec<_>>();
            assert!(
                rows.iter().any(|r| r.contains("Updated")),
                "{width}: {rows:?}"
            );
            let row = rows.iter().find(|r| r.contains("Untracked")).unwrap();
            assert!(row.contains("… Untracked "), "{width}: {row}");
            assert!(row.contains("15s"), "{width}: {row}");
            assert!(!row.contains("ago") && !row.contains('~'));
        }
    }

    #[test]
    fn full_visible_details_recover_request_and_model_above_compact_status_tail() {
        let mut app = content_app();
        app.details_area = Rect::new(65, 3, 35, 30);
        let source = format!(
            "Model: live-session-model\nReasoning: high\n› Submitted request above the status tail\n• Answer\n{}\n› draft\n70% context left",
            "visible response line\n".repeat(30)
        );
        let compact = source
            .lines()
            .rev()
            .take(24)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n");
        let observation = activity::Observation::parse(&compact, crate::process::Provider::Codex);
        assert!(observation.request.is_none() && observation.model.is_none());
        app.set_content(Some((
            "a".into(),
            Ok(content::parse(&source, crate::process::Provider::Codex)),
        )));
        let t = crate::theme::get("sunset").unwrap();
        let rendered = text(details(&app, &t));
        assert!(rendered.contains("live-session-model") && rendered.contains("high"));
        assert!(rendered.contains("Submitted request above"));
        assert!(!rendered.contains("draft"));
        let mut lines = Vec::new();
        detail_preview(&mut lines, "first second third fourth fifth", 12, 2, &t);
        let preview = text(lines);
        assert!(preview.starts_with("first second\n"));
        assert!(preview.ends_with('…') && !preview.ends_with("……"));
    }

    #[test]
    fn details_preview_is_bounded_and_technical_information_has_its_own_view() {
        let mut app = content_app();
        app.details_area = Rect::new(65, 3, 35, 30);
        let t = crate::theme::get("sunset").unwrap();
        let rendered = text(details(&app, &t));
        assert!(rendered.contains("User question only") && rendered.contains("Agent answer only"));
        assert!(!rendered.contains("END_OF_REPLY"));
        assert!(
            !rendered.contains("TERMINAL")
                && !rendered.contains("ACTIVITY")
                && !rendered.contains("f History")
        );
        assert!(rendered.find("PROJECT").unwrap() < rendered.find("LATEST REQUEST").unwrap());
        assert!(rendered.find("LATEST REQUEST").unwrap() < rendered.find("LATEST ANSWER").unwrap());
        let worker = inert_worker();
        let mut t = t;
        key(
            KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE),
            &mut app,
            &worker,
            &mut t,
        );
        let info = text(details(&app, &t));
        assert!(info.contains("DATA SOURCES") && info.contains("PROCESS"));
        assert!(!info.contains("LATEST ANSWER"));
        assert!(!key(
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
            &mut app,
            &worker,
            &mut t
        ));
        assert!(!app.technical);
        // A new provider request must not be paired with an old cached terminal answer.
        let mut p = Progress::default();
        p.native_metadata = true;
        p.request = Some("New pending request".into());
        app.activity.insert("a".into(), p);
        let rendered = text(details(&app, &t));
        assert!(rendered.contains("New pending request"));
        assert!(!rendered.contains("Agent answer only"));
    }

    #[test]
    fn dashboard_renders_timing_and_metadata_without_losing_history() {
        let mut app = content_app();
        let mut s = session("a", "/Users/user/Projects/ai-session-hub");
        s.location = Some(crate::iterm::Pane {
            id: "fixture-pane".into(),
            tty: "/dev/ttys001".into(),
            root_pid: 1,
            window_id: 1,
            window: "Agents".into(),
            tab: "AI Session Hub".into(),
            tab_index: 0,
            pane_index: 0,
            name: "Refresh session dashboard".into(),
        });
        app.reconcile(vec![s]);
        app.set_content(Some((
            "a".into(),
            Ok(content::parse(
                "› Keep the iteration history\n• Preserved\n› draft\n70% context left",
                crate::process::Provider::Codex,
            )),
        )));
        let now = SystemTime::now();
        let mut progress = Progress::default();
        progress.state = activity::State::Working;
        progress.model = Some("gpt-6.1-sol".into());
        progress.reasoning = Some("medium".into());
        progress.request = Some("Keep the iteration history".into());
        progress.updated_at = Some(now - Duration::from_secs(15));
        progress.requested_at = Some(now - Duration::from_secs(7200));
        progress.native_metadata = true;
        app.activity.insert("a".into(), progress);
        let t = crate::theme::get("calm").unwrap();
        let mut term = ratatui::Terminal::new(ratatui::backend::TestBackend::new(160, 36)).unwrap();
        term.draw(|f| draw(f, &mut app, &t)).unwrap();
        let rendered = (0..36)
            .map(|y| {
                (0..160)
                    .map(|x| term.backend().buffer()[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        for expected in [
            "Updated",
            "Requested",
            "Working",
            "15s",
            "gpt-6.1-sol",
            "medium",
            "Keep the iteration history",
        ] {
            assert!(rendered.contains(expected), "{expected}");
        }
        assert!(!rendered.contains("120m ago"));
        if std::env::var_os("AI_SESSION_HUB_PRINT_FIXTURE").is_some() {
            println!("{rendered}");
        }
        switch_view(&mut app, true);
        assert!(text(reader(&app, &t)).contains("Preserved"));
    }
    use crate::content::Role;
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
                    request: None,
                    model: None,
                    reasoning: None,
                    last_message: None,
                }),
            )],
            focused: None,
            metadata: Default::default(),
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
        assert!(sidebar.contains("◆"));
        assert!(sidebar.contains("New reply"));
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
            &format!("AI Session Hub v{}", env!("CARGO_PKG_VERSION")),
            "Model/reasoning not exposed",
            "LATEST REQUEST",
            "LATEST ANSWER",
        ] {
            assert!(text.contains(expected), "{expected}");
        }
        let selected: Vec<_> = buffer
            .content
            .iter()
            .filter(|c| c.bg == t.selected_bg && c.symbol() != " ")
            .collect();
        assert!(!selected.is_empty());
        assert!(selected.iter().any(|c| c.fg == t.selected_fg));
        assert!(!text.contains("LATEST EXCHANGE"));
        assert!(!text.contains("Git root"));
        assert!(!text.contains("Pane UUID"));
        assert!(text.contains("Answer"));
        assert!(!text.contains("TERMINAL"));
        switch_view(&mut app, true);
        term.draw(|f| draw(f, &mut app, &t)).unwrap();
        let rendered = term
            .backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect::<String>();
        assert!(rendered.contains("Request") && rendered.contains("Answer"));
        assert!(!rendered.contains("TERMINAL"));
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
                ..Default::default()
            }),
        ));
        app.history_view = true;
        app.detail_scroll = u16::MAX;
        let mut term = ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 25)).unwrap();
        term.draw(|f| draw(f, &mut app, &crate::theme::get("classic").unwrap()))
            .unwrap();
        assert!(app.detail_scroll > 0 && app.detail_scroll < u16::MAX);
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
                ..Default::default()
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
    fn recording_worker() -> (Worker, mpsc::Receiver<Request>, mpsc::Receiver<JumpRequest>) {
        let (tx, requests) = mpsc::channel();
        let (jumps, jump_requests) = mpsc::channel();
        let (_, rx) = mpsc::channel();
        (
            Worker {
                tx,
                jumps,
                rx,
                joins: vec![],
                cancelled: Arc::new(AtomicBool::new(false)),
            },
            requests,
            jump_requests,
        )
    }
    #[test]
    fn background_reads_follow_mode_and_independent_status_interval() {
        let mut app = content_app();
        let (worker, requests, _) = recording_worker();
        let config = Config::default();
        app.force_refresh = false;
        app.force_activity = false;
        app.last_scan = Some(Instant::now());
        app.last_activity = Some(Instant::now() - Duration::from_secs(6));
        app.selected_since = Instant::now() - Duration::from_secs(1);
        // Details loads once, even before entering History.
        poll_background(&mut app, &worker, &config);
        assert!(matches!(requests.try_recv(), Ok(Request::Content(s)) if s.id == "a"));
        assert!(requests.try_recv().is_err());
        app.content_pending = false;
        app.last_content = Some(Instant::now() - Duration::from_secs(6));
        poll_background(&mut app, &worker, &config);
        assert!(requests.try_recv().is_err());
        switch_view(&mut app, true);
        poll_background(&mut app, &worker, &config);
        assert!(matches!(requests.try_recv(), Ok(Request::Content(s)) if s.id == "a"));
        assert!(requests.try_recv().is_err());
        app.content_pending = false;
        app.last_content = Some(Instant::now());
        app.last_activity = Some(Instant::now() - Duration::from_secs(11));
        poll_background(&mut app, &worker, &config);
        assert!(matches!(requests.try_recv(), Ok(Request::Activity(s)) if s.len() == 2));
        assert!(requests.try_recv().is_err());
        switch_view(&mut app, false);
        app.last_content = Some(Instant::now() - Duration::from_secs(6));
        poll_background(&mut app, &worker, &config);
        assert!(requests.try_recv().is_err());
        // A new selection invalidates the preview without creating a periodic reader.
        app.move_selection(1);
        app.selected_since = Instant::now() - Duration::from_secs(1);
        poll_background(&mut app, &worker, &config);
        assert!(matches!(requests.try_recv(), Ok(Request::Content(s)) if s.id == "b"));
        assert!(requests.try_recv().is_err());
    }
    #[test]
    fn losing_focus_pauses_scans_and_returning_refreshes_even_after_inflight_work() {
        let mut app = content_app();
        let selected = app.row_key();
        let (worker, requests, jumps) = recording_worker();
        let config = Config::default();
        switch_view(&mut app, true);
        app.selected_since = Instant::now() - Duration::from_secs(1);
        app.set_active(false);
        poll_background(&mut app, &worker, &config);
        assert!(requests.try_recv().is_err());
        worker.jump(&mut app);
        assert!(matches!(jumps.try_recv(), Ok(JumpRequest::Jump(s)) if s.id == "a"));
        app.busy = true;
        app.content_pending = true;
        app.activity_pending = true;
        app.set_active(true);
        poll_background(&mut app, &worker, &config);
        assert!(requests.try_recv().is_err());
        assert!(app.force_refresh);
        app.busy = false;
        app.content_pending = false;
        app.activity_pending = false;
        // Late replies from before the pause must not suppress the fresh reads.
        app.last_content = Some(Instant::now());
        app.last_activity = Some(Instant::now());
        poll_background(&mut app, &worker, &config);
        assert!(matches!(requests.try_recv(), Ok(Request::Refresh(true))));
        assert!(matches!(requests.try_recv(), Ok(Request::Content(_))));
        assert!(matches!(requests.try_recv(), Ok(Request::Activity(_))));
        assert!(requests.try_recv().is_err());
        assert!(!app.force_refresh);
        assert!(app.history_view);
        assert_eq!(app.row_key(), selected);
    }
    #[test]
    fn user_idle_releases_both_helpers_and_input_warms_before_refresh() {
        let mut app = content_app();
        let selected = app.row_key();
        let (worker, requests, jumps) = recording_worker();
        switch_view(&mut app, true);
        app.last_user_activity = Instant::now() - HELPER_IDLE_TIMEOUT;
        poll_background(&mut app, &worker, &Config::default());
        assert!(app.sleeping);
        assert!(matches!(requests.try_recv(), Ok(Request::Sleep)));
        assert!(matches!(jumps.try_recv(), Ok(JumpRequest::Sleep)));
        poll_background(&mut app, &worker, &Config::default());
        assert!(requests.try_recv().is_err());
        assert!(jumps.try_recv().is_err());
        app.user_activity(&worker);
        assert!(!app.sleeping);
        assert!(matches!(requests.try_recv(), Ok(Request::Warm)));
        assert!(matches!(jumps.try_recv(), Ok(JumpRequest::Warm)));
        poll_background(&mut app, &worker, &Config::default());
        assert!(matches!(requests.try_recv(), Ok(Request::Refresh(true))));
        assert!(app.history_view);
        assert_eq!(app.row_key(), selected);
    }
    #[test]
    fn paired_views_preserve_session_and_allow_content_scrolling() {
        let mut app = content_app();
        let worker = inert_worker();
        let mut t = crate::theme::get("calm").unwrap();
        let press = |code, app: &mut App, t: &mut Theme| {
            assert!(!key(
                KeyEvent::new(code, KeyModifiers::NONE),
                app,
                &worker,
                t
            ));
        };
        let selected = app.row_key();
        assert_eq!(app.focus, Focus::Sessions);
        press(KeyCode::Char('f'), &mut app, &mut t);
        assert!(app.history_view);
        assert_eq!(app.focus, Focus::History);
        assert_eq!(app.row_key(), selected);
        let full = text(reader(&app, &t));
        assert!(
            full.contains("User question only")
                && full.contains("Tool action only")
                && full.contains("END_OF_REPLY")
        );
        press(KeyCode::Tab, &mut app, &mut t);
        assert_eq!(app.focus, Focus::Reader);
        press(KeyCode::Down, &mut app, &mut t);
        assert_eq!(app.detail_scroll, 1);
        press(KeyCode::BackTab, &mut app, &mut t);
        assert_eq!(app.focus, Focus::History);
        press(KeyCode::Enter, &mut app, &mut t);
        assert_eq!(app.focus, Focus::Reader);
        press(KeyCode::Char('f'), &mut app, &mut t);
        assert!(!app.history_view);
        assert_eq!(app.focus, Focus::Sessions);
        assert_eq!(app.row_key(), selected);
        press(KeyCode::Char('f'), &mut app, &mut t);
        press(KeyCode::Esc, &mut app, &mut t);
        assert!(!app.history_view);
        assert_eq!(app.focus, Focus::Sessions);
        press(KeyCode::Down, &mut app, &mut t);
        assert_eq!(app.selected().unwrap().id, "b");
    }
    #[test]
    fn iterations_group_requests_with_all_following_output_and_start_at_top() {
        let mut app = content_app();
        let mut content = selected_content(&app).unwrap().as_ref().unwrap().clone();
        content.messages.extend([
            content::Message {
                role: Role::You,
                text: "Second request".into(),
            },
            content::Message {
                role: Role::Agent,
                text: "Second answer".into(),
            },
            content::Message {
                role: Role::You,
                text: "Pending request".into(),
            },
        ]);
        app.set_content(Some(("a".into(), Ok(content.clone()))));
        let entries = history_entries(&app);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].messages.len(), 3);
        assert_eq!(entries[1].messages.len(), 2);
        assert_eq!(entries[2].messages.len(), 1);
        assert_eq!(app.history_state.selected(), Some(0));
        app.focus = Focus::History;
        switch_view(&mut app, true);
        app.move_history(1);
        let t = crate::theme::get("calm").unwrap();
        let full = text(reader(&app, &t));
        assert!(full.contains("Second request") && full.contains("Second answer"));
        assert!(!full.contains("User question only"));
        app.move_selection(1);
        assert!(!app.history_view);
        app.set_content(Some(("b".into(), Ok(content))));
        assert_eq!(app.history_state.selected(), Some(0));
        let mut term = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 30)).unwrap();
        term.draw(|f| draw(f, &mut app, &t)).unwrap();
        assert_eq!(app.history_state.offset(), 0);
    }
    #[test]
    fn previews_are_unicode_safe_and_refresh_preserves_the_reader() {
        assert_eq!(preview(&"я".repeat(101)), format!("{}…", "я".repeat(100)));
        assert_eq!(preview(&"я".repeat(100)), "я".repeat(100));
        let mut app = content_app();
        app.history_state.select(Some(0));
        app.focus = Focus::History;
        switch_view(&mut app, true);
        let mut next = selected_content(&app).unwrap().as_ref().unwrap().clone();
        next.messages[2].text.push_str(" Growing answer");
        app.set_content(Some(("a".into(), Ok(next.clone()))));
        assert!(app.history_view);
        assert_eq!(app.history_state.selected(), Some(0));
        next.messages.insert(
            0,
            content::Message {
                role: Role::Agent,
                text: "Earlier message".into(),
            },
        );
        app.set_content(Some(("a".into(), Ok(next.clone()))));
        assert_eq!(app.history_state.selected(), Some(1));
        assert!(app.history_view);
        app.focus = Focus::Reader;
        next.messages.remove(1);
        app.set_content(Some(("a".into(), Ok(next))));
        assert!(app.history_view);
        assert_eq!(app.focus, Focus::History);
        app.reconcile(vec![]);
        assert!(selected_entry(&app).is_none());
    }
    #[test]
    fn two_columns_resize_in_both_modes_and_mouse_preserves_focus() {
        let t = crate::theme::get("sunset").unwrap();
        let mut app = content_app();
        for (w, h) in [(150, 40), (100, 25), (80, 25), (50, 12), (150, 40)] {
            for history in [false, true] {
                switch_view(&mut app, history);
                let mut term =
                    ratatui::Terminal::new(ratatui::backend::TestBackend::new(w, h)).unwrap();
                term.draw(|f| draw(f, &mut app, &t)).unwrap();
                let left = if history {
                    app.history_area
                } else {
                    app.list_area
                };
                let hidden = if history {
                    app.list_area
                } else {
                    app.history_area
                };
                assert_eq!(hidden, Rect::default());
                for area in [left, app.details_area] {
                    assert!(area.width > 2 && area.height >= 3, "{w}×{h}: {area:?}");
                    assert!(area.right() <= w && area.bottom() <= h - 2);
                }
                assert_eq!(left.y, app.details_area.y);
                assert_eq!(left.height, app.details_area.height);
                assert_eq!(left.right(), app.details_area.x);
                assert!(!left.intersects(app.details_area));
                let rendered = (0..h)
                    .map(|y| {
                        (0..w)
                            .map(|x| term.backend().buffer()[(x, y)].symbol())
                            .collect::<String>()
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                if history {
                    assert!(rendered.contains("Folder:"));
                    assert!(rendered.contains("Content"));
                    assert!(!rendered.contains("TERMINAL"));
                } else {
                    assert!(rendered.contains("Sessions"));
                    assert!(rendered.contains("Details"));
                    assert!(!rendered.contains("END_OF_REPLY"));
                }
                let before = app.row_key();
                let focus = app.focus;
                mouse(
                    event::MouseEvent {
                        kind: MouseEventKind::Down(event::MouseButton::Left),
                        column: left.x + 2,
                        row: left.y + 1,
                        modifiers: KeyModifiers::NONE,
                    },
                    &mut app,
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
                assert_eq!(app.focus, focus);
                assert_eq!(app.row_key(), before);
            }
        }
    }
    #[test]
    fn empty_history_and_search_keep_mode_and_selection_consistent() {
        let mut app = content_app();
        app.content = None;
        switch_view(&mut app, true);
        let t = crate::theme::get("calm").unwrap();
        let mut term = ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 25)).unwrap();
        term.draw(|f| draw(f, &mut app, &t)).unwrap();
        assert!(app.history_view);
        assert_eq!(app.focus, Focus::History);
        let worker = inert_worker();
        let mut t = t;
        key(
            KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE),
            &mut app,
            &worker,
            &mut t,
        );
        assert!(app.editing);
        assert!(!app.history_view);
        assert_eq!(app.focus, Focus::Sessions);
    }
    #[test]
    fn compact_paths_keep_parent_and_home_identity() {
        assert_eq!(
            compact_path_with_home("/Users/alex", Some("/Users/alex")),
            "~"
        );
        assert_eq!(
            compact_path_with_home("/Users/alex/ai-session-hub", Some("/Users/alex")),
            "ai-session-hub"
        );
        assert_eq!(
            compact_path_with_home("/work/cli/ai-session-hub", Some("/Users/alex")),
            "ai-session-hub"
        );
        assert_eq!(
            compact_path_with_home("/Users/alexander/project", Some("/Users/alex")),
            "project"
        );
    }
}
