use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::{process::Command, time::Duration};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pane {
    pub id: String,
    pub tty: String,
    pub root_pid: u32,
    pub job_pid: u32,
    pub window_id: i64,
    pub window: String,
    pub tab: String,
    pub tab_index: usize,
    pub pane_index: usize,
    pub name: String,
}

const SCRIPT: &str = include_str!("iterm.js");

pub fn call(args: &[&str]) -> Result<String> {
    let mut command = Command::new("/usr/bin/osascript");
    command.args(["-l", "JavaScript", "-e", SCRIPT]).args(args);
    crate::command::output(command, Duration::from_secs(5))
}

pub fn panes() -> Result<Vec<Pane>> {
    Ok(serde_json::from_str(&call(&["list"])?)?)
}

pub fn jump(pane: &Pane) -> Result<()> {
    let response = call(&[
        "jump",
        &pane.id,
        &pane.tty,
        &pane.root_pid.to_string(),
        &pane.job_pid.to_string(),
    ])?;
    if response.trim() != pane.id {
        bail!("Selected pane could not be confirmed");
    }
    Ok(())
}
