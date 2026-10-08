use crate::{
    iterm::{self, Pane},
    process::{self, Process, Provider},
};
use anyhow::{Result, bail};
use serde::Serialize;
use std::os::unix::fs::MetadataExt;
use std::{collections::HashMap, process::Command, time::Duration};

#[derive(Debug, Clone, Serialize)]
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

pub fn collect() -> Result<Vec<Session>> {
    let table = process::scan()?;
    let (panes, error) = match iterm::panes() {
        Ok(p) => (p, None),
        Err(e) => (vec![], Some(format!("iTerm2 unavailable: {e}"))),
    };
    let agents = process::agents(&table);
    if agents.is_empty()
        && let Some(e) = error.as_ref()
    {
        bail!("{e}");
    }
    let mut roots = HashMap::new();
    let mut sessions = Vec::new();
    for p in &agents {
        let matches: Vec<_> = panes
            .iter()
            .filter(|pane| linked(p, pane, &table))
            .collect();
        let ambiguous = matches.len() > 1
            || (matches.len() == 1
                && agents
                    .iter()
                    .filter(|agent| linked(agent, matches[0], &table))
                    .count()
                    != 1);
        let location = (matches.len() == 1 && !ambiguous).then(|| matches[0].clone());
        let reason = if location.is_some() {
            None
        } else {
            Some(error.clone().unwrap_or_else(|| {
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
    Ok(sessions)
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

pub fn jump(selected: &Session) -> Result<()> {
    let current = collect()?
        .into_iter()
        .find(|s| s.id == selected.id)
        .ok_or_else(|| anyhow::anyhow!("Session exited; refresh the list"))?;
    let pane = current.location.as_ref().ok_or_else(|| {
        anyhow::anyhow!(
            "Unlinked: {}",
            current.link_reason.as_deref().unwrap_or("Unknown pane")
        )
    })?;
    if selected
        .location
        .as_ref()
        .is_some_and(|old| old.id != pane.id)
    {
        bail!("Pane identity changed; refresh before Jump");
    }
    // Re-read the process immediately before sending the terminal selection.
    let table = process::scan()?;
    if !process::agents(&table)
        .iter()
        .any(|p| p.pid == current.pid && p.started == current.started && linked(p, pane, &table))
    {
        bail!("Session linkage changed before Jump");
    }
    iterm::jump(pane)
}
