use anyhow::{Context, Result, bail};
use std::{
    io::Read,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

pub fn output(mut command: Command, timeout: Duration) -> Result<String> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Cannot start helper")?;
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let out = thread::spawn(move || {
        let mut s = Vec::new();
        stdout.read_to_end(&mut s).map(|_| s)
    });
    let err = thread::spawn(move || {
        let mut s = Vec::new();
        stderr.read_to_end(&mut s).map(|_| s)
    });
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break Some(s),
            Ok(None) if start.elapsed() < timeout => thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let stdout = out
        .join()
        .map_err(|_| anyhow::anyhow!("Helper reader failed"))??;
    let stderr = err
        .join()
        .map_err(|_| anyhow::anyhow!("Helper reader failed"))??;
    let Some(status) = status else {
        bail!("Helper timed out");
    };
    if !status.success() {
        bail!("{}", String::from_utf8_lossy(&stderr).trim());
    }
    String::from_utf8(stdout).context("Invalid helper output")
}
