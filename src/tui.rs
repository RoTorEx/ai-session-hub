use crate::{
    config::Config,
    model::{self, Session, safe_text},
    theme::Theme,
};
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};
use std::{
    collections::{BTreeMap, HashSet},
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
    Project(String, usize),
    Session(usize),
}
pub struct App {
    sessions: Vec<Session>,
    rows: Vec<Row>,
    state: ListState,
    collapsed: HashSet<String>,
    filter: String,
    editing: bool,
    details_focus: bool,
    detail_scroll: u16,
    help: bool,
    notice: String,
    stale: bool,
    busy: bool,
    jump_pending: bool,
    last_scan: Option<Instant>,
}
impl App {
    fn new() -> Self {
        Self {
            sessions: vec![],
            rows: vec![],
            state: ListState::default(),
            collapsed: HashSet::new(),
            filter: String::new(),
            editing: false,
            details_focus: false,
            detail_scroll: 0,
            help: false,
            notice: "Scanning sessions…".into(),
            stale: false,
            busy: false,
            jump_pending: false,
            last_scan: None,
        }
    }
    fn selected(&self) -> Option<&Session> {
        match self.rows.get(self.state.selected()?)? {
            Row::Session(i) => self.sessions.get(*i),
            _ => None,
        }
    }
    fn row_key(&self) -> Option<String> {
        match self.rows.get(self.state.selected()?)? {
            Row::Project(p, _) => Some(format!("project:{p}")),
            Row::Session(i) => Some(self.sessions[*i].id.clone()),
        }
    }
    fn rebuild(&mut self, preferred: Option<String>) {
        let old_index = self.state.selected().unwrap_or(0);
        let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        let needle = self.filter.to_lowercase();
        for (i, s) in self.sessions.iter().enumerate() {
            let titles = s
                .location
                .as_ref()
                .map(|p| format!("{} {} {}", p.name, p.tab, p.window))
                .unwrap_or_default();
            let text = format!(
                "{} {} {} {}",
                s.provider,
                s.project,
                s.cwd.as_deref().unwrap_or(""),
                titles
            )
            .to_lowercase();
            if text.contains(&needle) {
                groups.entry(s.project.clone()).or_default().push(i);
            }
        }
        self.rows.clear();
        for (p, indexes) in groups {
            self.rows.push(Row::Project(p.clone(), indexes.len()));
            if !self.collapsed.contains(&p) {
                self.rows.extend(indexes.into_iter().map(Row::Session));
            }
        }
        let selected = preferred
            .and_then(|key| {
                self.rows.iter().position(|r| match r {
                    Row::Project(p, _) => key == format!("project:{p}"),
                    Row::Session(i) => self.sessions[*i].id == key,
                })
            })
            .or_else(|| {
                (!self.rows.is_empty()).then_some(old_index.min(self.rows.len().saturating_sub(1)))
            });
        self.state.select(selected);
    }
    fn reconcile(&mut self, sessions: Vec<Session>) {
        let key = self.row_key();
        self.sessions = sessions;
        self.rebuild(key.clone());
        if self.row_key() != key {
            self.detail_scroll = 0;
        }
        if self.last_scan.is_none()
            && let Some(i) = self.rows.iter().position(|r| matches!(r, Row::Session(_)))
        {
            self.state.select(Some(i));
        }
        self.last_scan = Some(Instant::now());
        self.stale = false;
    }
    fn move_selection(&mut self, delta: isize) {
        if self.rows.is_empty() {
            return;
        }
        let next = self
            .state
            .selected()
            .unwrap_or(0)
            .saturating_add_signed(delta)
            .min(self.rows.len() - 1);
        if self.state.selected() != Some(next) {
            self.state.select(Some(next));
            self.detail_scroll = 0;
        }
    }
    fn toggle(&mut self, collapse: Option<bool>) {
        let Some(Row::Project(p, _)) = self
            .state
            .selected()
            .and_then(|i| self.rows.get(i))
            .cloned()
        else {
            return;
        };
        let collapse = collapse.unwrap_or(!self.collapsed.contains(&p));
        if collapse {
            self.collapsed.insert(p);
        } else {
            self.collapsed.remove(&p);
        }
        self.rebuild(self.row_key());
    }
}
enum Request {
    Refresh,
    Jump(Box<Session>),
    Stop,
}
enum Response {
    Refreshed(Result<Vec<Session>>),
    Jumped(Result<()>),
}
struct Worker {
    tx: mpsc::Sender<Request>,
    rx: mpsc::Receiver<Response>,
    join: Option<thread::JoinHandle<()>>,
    cancelled: Arc<AtomicBool>,
}
impl Worker {
    fn start() -> Self {
        let (tx, requests) = mpsc::channel();
        let (results, rx) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let stopping = cancelled.clone();
        let join = thread::spawn(move || {
            while let Ok(request) = requests.recv() {
                if stopping.load(Ordering::Relaxed) {
                    break;
                }
                let response = match request {
                    Request::Refresh => Response::Refreshed(model::collect()),
                    Request::Jump(s) => Response::Jumped(model::jump(&s)),
                    Request::Stop => break,
                };
                if results.send(response).is_err() {
                    break;
                }
            }
        });
        Self {
            tx,
            rx,
            join: Some(join),
            cancelled,
        }
    }
    fn send(&self, request: Request, app: &mut App) {
        let jumping = matches!(request, Request::Jump(_));
        if jumping && app.jump_pending {
            return;
        }
        if (jumping || !app.busy) && self.tx.send(request).is_ok() {
            app.busy = true;
            if jumping {
                app.jump_pending = true;
                app.notice = "Checking exact pane linkage…".into();
            }
        }
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
        let _ = self.tx.send(Request::Stop);
        if let Some(h) = self.join.take() {
            let _ = h.join();
        }
    }
}
struct TerminalGuard;
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        ratatui::restore();
    }
}

pub fn run(config: Config, theme: Theme) -> Result<()> {
    let terminate = Arc::new(AtomicBool::new(false));
    let mut signals = Vec::new();
    for sig in [signal_hook::consts::SIGTERM, signal_hook::consts::SIGINT] {
        signals.push(signal_hook::flag::register(sig, terminate.clone())?);
    }
    let result = (|| -> Result<()> {
        // Worker is dropped after terminal guard, restoring the terminal before joining.
        let worker = Worker::start();
        let mut terminal = ratatui::try_init()?;
        let _guard = TerminalGuard;
        let mut app = App::new();
        worker.send(Request::Refresh, &mut app);
        while !terminate.load(Ordering::Relaxed) {
            while let Ok(response) = worker.rx.try_recv() {
                app.busy = app.jump_pending;
                match response {
                    Response::Refreshed(Ok(s)) => {
                        app.reconcile(s);
                        app.notice = "".into();
                    }
                    Response::Refreshed(Err(e)) => {
                        app.notice = format!("Refresh failed: {e}");
                        app.stale = true;
                        app.last_scan = Some(Instant::now());
                    }
                    Response::Jumped(result) => {
                        app.busy = false;
                        app.jump_pending = false;
                        app.notice = match result {
                            Ok(()) => "Selected existing iTerm2 pane".into(),
                            Err(e) => format!("Jump refused: {e}"),
                        };
                    }
                }
            }
            if !app.busy
                && app
                    .last_scan
                    .is_some_and(|t| t.elapsed() >= Duration::from_secs(config.refresh_seconds))
            {
                worker.send(Request::Refresh, &mut app);
            }
            terminal.draw(|f| draw(f, &mut app, &theme))?;
            if event::poll(Duration::from_millis(100))? {
                match event::read()? {
                    Event::Key(k)
                        if k.kind == event::KeyEventKind::Press && key(k, &mut app, &worker) =>
                    {
                        break;
                    }
                    Event::Resize(_, _) => {}
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
        match k.code {
            KeyCode::Esc => {
                app.editing = false;
                app.filter.clear();
                app.rebuild(None);
            }
            KeyCode::Enter => app.editing = false,
            KeyCode::Backspace => {
                app.filter.pop();
                app.rebuild(None);
            }
            KeyCode::Char(c) => {
                app.filter.push(c);
                app.rebuild(None);
            }
            _ => {}
        }
        return false;
    }
    if app.help {
        if matches!(k.code, KeyCode::Char('q' | 'й')) {
            return true;
        }
        if matches!(k.code, KeyCode::Esc | KeyCode::Char('?')) {
            app.help = false;
        }
        return false;
    }
    match k.code {
        KeyCode::Char('q' | 'й') | KeyCode::Esc => return true,
        KeyCode::Char('/') => {
            app.editing = true;
            app.detail_scroll = 0;
        }
        KeyCode::Char('?') => app.help = true,
        KeyCode::Tab => app.details_focus = !app.details_focus,
        KeyCode::Char('r' | 'к') => worker.send(Request::Refresh, app),
        KeyCode::Char('o' | 'щ') | KeyCode::Enter => {
            if let Some(s) = app.selected().cloned() {
                worker.send(Request::Jump(Box::new(s)), app);
            } else {
                app.toggle(None);
            }
        }
        KeyCode::Left if !app.details_focus => app.toggle(Some(true)),
        KeyCode::Right if !app.details_focus => app.toggle(Some(false)),
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
        KeyCode::End if !app.details_focus => app.move_selection(isize::MAX),
        _ => {}
    }
    false
}
fn details(s: &Session) -> String {
    let pane = if let Some(p) = &s.location {
        format!(
            "Window: {}\nWindow ID: {}\nTab: {} (#{})\nPane: {} (#{})\nPane UUID: {}\nTTY: {}",
            p.window,
            p.window_id,
            p.tab,
            p.tab_index + 1,
            p.name,
            p.pane_index + 1,
            p.id,
            p.tty
        )
    } else {
        format!(
            "Link: Unlinked\nReason: {}",
            s.link_reason.as_deref().unwrap_or("Unavailable")
        )
    };
    format!(
        "Agent: {}\nState: Live\nLink: {}\n\nProject: {}\nWorking directory: {}\n\nSession ID: {}\nPID: {}\nProcess start (Unix µs): {}\n\n{}\n\nDescription: Unavailable\nConversation ID: Unavailable\nActivity details: Unavailable\n\nJump revalidates process identity and exact pane linkage.",
        s.provider,
        if s.location.is_some() {
            "Linked"
        } else {
            "Unlinked"
        },
        s.project,
        s.cwd.as_deref().unwrap_or("Unavailable"),
        s.id,
        s.pid,
        s.started,
        pane
    )
}
fn project_label(path: &str) -> String {
    let p = std::path::Path::new(path);
    let name = p
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.into());
    let parent = p
        .parent()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    safe_text(&format!("{name} · {parent}"))
}
fn draw(f: &mut Frame, app: &mut App, t: &Theme) {
    let area = f.area();
    if area.width < 50 || area.height < 10 {
        f.render_widget(
            Paragraph::new("Resize terminal to at least 50 × 10. q to quit."),
            area,
        );
        return;
    }
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(2),
        ])
        .split(area);
    let freshness = if app.stale {
        "STALE"
    } else if app.busy {
        "Updating"
    } else {
        "Live"
    };
    f.render_widget(
        Paragraph::new(format!(
            "FleetScope {} · {} · {} · Filter: {}{}",
            env!("CARGO_PKG_VERSION"),
            t.name,
            freshness,
            safe_text(&app.filter),
            if app.editing { "_" } else { "" }
        ))
        .style(Style::default().fg(t.status)),
        outer[0],
    );
    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
        .split(outer[1]);
    let items: Vec<_> = app
        .rows
        .iter()
        .map(|row| match row {
            Row::Project(p, count) => ListItem::new(format!(
                "{}{} [{}] {}",
                if app.collapsed.contains(p) {
                    "▸ "
                } else {
                    "▾ "
                },
                t.project_icon,
                count,
                project_label(p)
            ))
            .style(Style::default().fg(t.project).add_modifier(Modifier::BOLD)),
            Row::Session(i) => {
                let s = &app.sessions[*i];
                let name = s
                    .location
                    .as_ref()
                    .map(|p| p.name.as_str())
                    .unwrap_or("Unlinked");
                ListItem::new(format!(
                    "{}{} #{} {}",
                    t.task_icon,
                    s.provider,
                    s.pid,
                    safe_text(name)
                ))
                .style(Style::default().fg(t.task))
            }
        })
        .collect();
    let border = |focus: bool, c| {
        if focus {
            Style::default().fg(c).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(c)
        }
    };
    let list = List::new(items)
        .block(
            Block::default()
                .title(t.sessions_title.as_str())
                .borders(Borders::ALL)
                .border_style(border(!app.details_focus, t.list_border)),
        )
        .highlight_style(Style::default().fg(t.selected_fg).bg(t.selected_bg))
        .scroll_padding(2);
    f.render_stateful_widget(list, panes[0], &mut app.state);
    if app.rows.is_empty() {
        f.render_widget(
            Paragraph::new(if app.busy {
                "Scanning…"
            } else if app.filter.is_empty() {
                "No live Codex/Claude sessions"
            } else {
                "No matching sessions"
            })
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(t.sessions_title.as_str()),
            ),
            panes[0],
        );
    }
    let text = app.selected().map(details).unwrap_or_else(|| {
        "Select a session. Enter/←/→ on a project expands or collapses it.".into()
    });
    // Sanitize control characters but preserve the layout's own newline separators.
    let text = text.lines().map(safe_text).collect::<Vec<_>>().join("\n");
    let block = Block::default()
        .title(t.details_title.as_str())
        .borders(Borders::ALL)
        .border_style(border(app.details_focus, t.detail_border));
    let paragraph = Paragraph::new(text).wrap(Wrap { trim: false });
    let max_scroll = paragraph
        .line_count(panes[1].width.saturating_sub(2).max(1))
        .saturating_sub(panes[1].height.saturating_sub(2) as usize)
        .min(u16::MAX as usize) as u16;
    app.detail_scroll = app.detail_scroll.min(max_scroll);
    f.render_widget(
        paragraph
            .block(block)
            .scroll((app.detail_scroll, 0))
            .style(Style::default().fg(t.label)),
        panes[1],
    );
    let status = if app.notice.is_empty() {
        format!(
            "Tab focus {} ↑↓/jk move {} Enter/o Jump {} / search {} r refresh {} ? help {} q quit",
            t.help_separator,
            t.help_separator,
            t.help_separator,
            t.help_separator,
            t.help_separator,
            t.help_separator
        )
    } else {
        format!(
            "{} {}",
            if app.stale || app.notice.contains("refused") {
                &t.error_title
            } else {
                &t.notice_title
            },
            safe_text(&app.notice)
        )
    };
    f.render_widget(
        Paragraph::new(status)
            .wrap(Wrap { trim: true })
            .style(Style::default().fg(if app.stale { t.error } else { t.info })),
        outer[2],
    );
    if app.help {
        f.render_widget(ratatui::widgets::Clear, panes[1]);
        f.render_widget(Paragraph::new("↑/↓ or j/k: move in focused pane\nTab: switch tree/details focus\n←/→: collapse/expand project\nPageUp/PageDown: page focused pane\nEnter/o: exact existing-pane Jump\n/: filter project/agent/path/titles\nEnter: accept filter; Esc: clear it\nr: refresh; ?: help; q/Ctrl+C: quit\nEsc: close help, otherwise quit\n\nFleetScope never types into agents.\nLive includes waiting for input.\nUnlinked sessions cannot be opened.").block(Block::default().title("Help · ? / Esc to close").borders(Borders::ALL)).wrap(Wrap{trim:false}),panes[1]);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn session(id: &str) -> Session {
        Session {
            id: id.into(),
            pid: 1,
            started: 1,
            provider: crate::process::Provider::Codex,
            cwd: Some("/project".into()),
            project: "/project".into(),
            status: "Live",
            location: None,
            link_reason: Some("Unknown".into()),
        }
    }
    #[test]
    fn refresh_preserves_selected_identity_and_independent_scroll() {
        let mut app = App::new();
        app.reconcile(vec![session("a"), session("b")]);
        app.move_selection(1);
        app.detail_scroll = 3;
        app.reconcile(vec![session("b"), session("a")]);
        assert_eq!(app.selected().unwrap().id, "b");
        assert_eq!(app.detail_scroll, 3);
        app.reconcile(vec![session("a")]);
        assert_eq!(app.selected().unwrap().id, "a");
        assert_eq!(app.detail_scroll, 0);
    }
    #[test]
    fn filter_and_collapsed_groups_survive_refresh() {
        let mut app = App::new();
        app.reconcile(vec![session("a")]);
        app.state.select(Some(0));
        app.toggle(Some(true));
        app.reconcile(vec![session("a"), session("b")]);
        assert_eq!(app.rows.len(), 1);
        app.filter = "claude".into();
        app.rebuild(None);
        assert!(app.rows.is_empty());
    }
    #[test]
    fn renders_small_and_empty_terminal_without_panicking() {
        for (w, h) in [(20, 5), (100, 25)] {
            let mut term =
                ratatui::Terminal::new(ratatui::backend::TestBackend::new(w, h)).unwrap();
            let mut app = App::new();
            term.draw(|f| draw(f, &mut app, &crate::theme::get("classic").unwrap()))
                .unwrap();
        }
    }
}
