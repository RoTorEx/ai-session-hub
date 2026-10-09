use crate::{
    iterm::{self, Pane},
    process::{self, Process, Provider},
};
use anyhow::{Result, bail};
use serde::Serialize;
use std::os::unix::fs::MetadataExt;
use std::{
    collections::HashMap,
    process::Command,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Session {
    pub id: String,
    pub pid: u32,
    pub started: u64,
    pub provider: Provider,
    pub cwd: Option<String>,
    pub project: String,
    pub status: &'static str,
    pub location: Option<Pane>,
    pub link_reason: Option<String>,
}

fn linked(process: &Process, pane: &Pane, table: &[Process]) -> bool {
    std::fs::metadata(&pane.tty).is_ok_and(|m| m.rdev() as u32 == process.tty)
        && process::descendant(process.pid, pane.root_pid, table)
}

pub struct Collector {
    pub terminal: iterm::Client,
    panes: Vec<Pane>,
    error: Option<String>,
    identities: Vec<(u32, u64)>,
    last_metadata: Option<Instant>,
    roots: HashMap<String, String>,
    metadata_interval: Duration,
}
impl Collector {
    pub fn new(metadata_seconds: u64) -> Self {
        Self {
            terminal: iterm::Client::default(),
            panes: vec![],
            error: None,
            identities: vec![],
            last_metadata: None,
            roots: HashMap::new(),
            metadata_interval: Duration::from_secs(metadata_seconds),
        }
    }
    pub fn scan(&mut self, force: bool) -> Result<Vec<Session>> {
        let table = process::scan()?;
        let agents = process::agents(&table);
        let mut ids: Vec<_> = agents.iter().map(|p| (p.pid, p.started)).collect();
        ids.sort_unstable();
        if force
            || ids.iter().any(|id| !self.identities.contains(id))
            || self
                .last_metadata
                .is_none_or(|t| t.elapsed() >= self.metadata_interval)
        {
            match self.terminal.panes() {
                Ok(p) => {
                    self.panes = p;
                    self.error = None;
                }
                Err(e) => {
                    self.panes.clear();
                    self.error = Some(format!("iTerm2 unavailable: {e}"));
                }
            }
            self.last_metadata = Some(Instant::now());
            self.roots.clear();
        }
        self.identities = ids;
        if agents.is_empty()
            && let Some(e) = self.error.as_ref()
        {
            bail!("{e}");
        }
        Ok(assemble(
            &table,
            &agents,
            &self.panes,
            self.error.as_deref(),
            &mut self.roots,
        ))
    }
}

pub fn collect() -> Result<Vec<Session>> {
    Collector::new(30).scan(true)
}

fn assemble(
    table: &[Process],
    agents: &[Process],
    panes: &[Pane],
    error: Option<&str>,
    roots: &mut HashMap<String, String>,
) -> Vec<Session> {
    let mut sessions = Vec::new();
    for p in agents {
        let matches: Vec<_> = panes.iter().filter(|pane| linked(p, pane, table)).collect();
        let ambiguous = matches.len() > 1
            || (matches.len() == 1
                && agents
                    .iter()
                    .filter(|agent| linked(agent, matches[0], table))
                    .count()
                    != 1);
        let location = (matches.len() == 1 && !ambiguous).then(|| matches[0].clone());
        let reason = if location.is_some() {
            None
        } else {
            Some(error.map(str::to_owned).unwrap_or_else(|| {
                if ambiguous {
                    "Ambiguous pane linkage"
                } else {
                    "No confirmed iTerm2 pane"
                }
                .into()
            }))
        };
        let cwd = p.cwd.as_ref().map(|c| c.to_string_lossy().into_owned());
        let project = cwd
            .as_ref()
            .map(|path| {
                roots
                    .entry(path.clone())
                    .or_insert_with(|| project_root(path))
                    .clone()
            })
            .unwrap_or_else(|| "Unknown directory".into());
        sessions.push(Session {
            id: format!("{}-{}", p.pid, p.started),
            pid: p.pid,
            started: p.started,
            provider: p.provider.unwrap(),
            project,
            cwd,
            status: "Live",
            location,
            link_reason: reason,
        });
    }
    sessions.sort_by(|a, b| (&a.project, a.pid).cmp(&(&b.project, b.pid)));
    sessions
}

fn project_root(cwd: &str) -> String {
    let mut cmd = Command::new("/usr/bin/git");
    cmd.args(["-C", cwd, "rev-parse", "--show-toplevel"]);
    crate::command::output(cmd, Duration::from_secs(1))
        .ok()
        .map(|s| s.trim_end_matches('\n').to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| cwd.into())
}

pub fn safe_text(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

pub fn validate(selected: &Session) -> Result<&Pane> {
    let pane = selected.location.as_ref().ok_or_else(|| {
        anyhow::anyhow!(
            "Unlinked: {}",
            selected.link_reason.as_deref().unwrap_or("Unknown pane")
        )
    })?;
    let table = process::scan()?;
    if !valid_identity(selected, pane, &table) {
        bail!("Session exited or linkage changed; refresh the list");
    }
    Ok(pane)
}

pub fn jump_with(selected: &Session, terminal: &mut iterm::Client) -> Result<()> {
    terminal.jump(validate(selected)?)
}

pub(crate) fn valid_identity(selected: &Session, pane: &Pane, table: &[Process]) -> bool {
    let mut agents = process::agents(table)
        .into_iter()
        .filter(|p| linked(p, pane, table));
    agents
        .next()
        .is_some_and(|p| p.pid == selected.pid && p.started == selected.started)
        && agents.next().is_none()
}
#[cfg(test)]
mod tests {
    use super::*;
    fn agent(pid: u32) -> Process {
        let tty = std::fs::metadata("/dev/null").unwrap().rdev() as u32;
        Process {
            pid,
            parent: 7,
            started: 99,
            tty,
            stdin_tty: Some(tty),
            cwd: None,
            provider: Some(Provider::Codex),
            launcher: false,
        }
    }
    fn pane() -> Pane {
        Pane {
            id: "uuid".into(),
            tty: "/dev/null".into(),
            root_pid: 7,
            window_id: 1,
            window: String::new(),
            tab: String::new(),
            tab_index: 0,
            pane_index: 0,
            name: String::new(),
        }
    }
    fn root() -> Process {
        let mut p = agent(7);
        p.parent = 1;
        p.provider = None;
        p
    }
    #[test]
    fn fast_jump_rejects_pid_reuse_and_changed_root() {
        let p = agent(8);
        let pane = pane();
        let table = vec![root(), p.clone()];
        let s = assemble(
            &table,
            std::slice::from_ref(&p),
            std::slice::from_ref(&pane),
            None,
            &mut HashMap::new(),
        )
        .remove(0);
        assert!(valid_identity(&s, &pane, &table));
        assert!(!valid_identity(&s, &pane, &[root(), agent(8), agent(9)]));
        let mut reused = p;
        reused.started += 1;
        assert!(!valid_identity(&s, &pane, &[root(), reused]));
        let mut other = pane;
        other.root_pid = 100;
        assert!(!valid_identity(&s, &other, &table));
    }
    #[test]
    fn multiple_independent_agents_in_one_pane_remain_unlinked() {
        let agents = vec![agent(8), agent(9)];
        let mut table = agents.clone();
        table.push(root());
        let sessions = assemble(&table, &agents, &[pane()], None, &mut HashMap::new());
        assert_eq!(sessions.len(), 2);
        assert!(
            sessions.iter().all(|s| s.location.is_none()
                && s.link_reason.as_deref() == Some("Ambiguous pane linkage"))
        );
    }
}
