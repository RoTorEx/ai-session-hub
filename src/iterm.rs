use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc,
    thread,
    time::Duration,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pane {
    pub id: String,
    pub tty: String,
    pub root_pid: u32,
    pub window_id: i64,
    pub window: String,
    pub tab: String,
    pub tab_index: usize,
    pub pane_index: usize,
    pub name: String,
}
const SCRIPT: &str = include_str!("iterm.js");
struct Helper {
    child: Child,
    input: ChildStdin,
    replies: mpsc::Receiver<String>,
    reader: Option<thread::JoinHandle<()>>,
}
impl Drop for Helper {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(h) = self.reader.take() {
            let _ = h.join();
        }
    }
}
#[derive(Default)]
pub struct Client {
    helper: Option<Helper>,
}
impl Client {
    fn call(&mut self, args: &[&str]) -> Result<String> {
        if self.helper.is_none() {
            let mut child = Command::new("/usr/bin/osascript")
                .args(["-l", "JavaScript", "-e", SCRIPT, "server"])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .context("Cannot start iTerm2 helper")?;
            let input = child.stdin.take().unwrap();
            let output = child.stdout.take().unwrap();
            let (tx, replies) = mpsc::channel();
            let reader = thread::spawn(move || {
                for line in BufReader::new(output).lines() {
                    match line {
                        Ok(s) => {
                            if tx.send(s).is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            });
            self.helper = Some(Helper {
                child,
                input,
                replies,
                reader: Some(reader),
            });
        }
        let result = (|| -> Result<String> {
            let helper = self.helper.as_mut().unwrap();
            writeln!(helper.input, "{}", serde_json::to_string(args)?)?;
            helper.input.flush()?;
            helper
                .replies
                .recv_timeout(Duration::from_secs(5))
                .context("iTerm2 helper stopped or timed out")
        })();
        let line = match result {
            Ok(s) => s,
            Err(e) => {
                self.helper.take();
                return Err(e);
            }
        };
        let response: serde_json::Value =
            serde_json::from_str(&line).context("Invalid iTerm2 helper response")?;
        if response["ok"] != true {
            bail!(
                "{}",
                response["error"]
                    .as_str()
                    .unwrap_or("iTerm2 request failed")
            );
        }
        Ok(response["value"]
            .as_str()
            .context("Invalid iTerm2 result")?
            .to_owned())
    }
    pub fn warm(&mut self) -> Result<()> {
        self.call(&["ping"])?;
        Ok(())
    }
    pub fn panes(&mut self) -> Result<Vec<Pane>> {
        Ok(serde_json::from_str(&self.call(&["list"])?)?)
    }
    fn pane_call(&mut self, action: &str, pane: &Pane) -> Result<String> {
        self.call(&[
            action,
            &pane.id,
            &pane.tty,
            &pane.root_pid.to_string(),
            &pane.window_id.to_string(),
            &pane.tab_index.to_string(),
            &pane.pane_index.to_string(),
        ])
    }
    pub fn jump(&mut self, pane: &Pane) -> Result<()> {
        if self.pane_call("jump", pane)?.trim() != pane.id {
            bail!("Selected pane could not be confirmed");
        }
        Ok(())
    }
    pub fn content(&mut self, pane: &Pane) -> Result<String> {
        self.pane_call("content", pane)
    }
}
