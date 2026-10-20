use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::{
    env, fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Config {
    pub theme: String,
    pub refresh_seconds: u64,
    pub metadata_seconds: u64,
    pub content_seconds: u64,
    pub activity_seconds: u64,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            theme: "calm".into(),
            refresh_seconds: 2,
            metadata_seconds: 30,
            content_seconds: 5,
            activity_seconds: 10,
        }
    }
}

pub fn path() -> Result<PathBuf> {
    if let Some(p) = env::var_os("AI_SESSION_HUB_CONFIG") {
        return Ok(p.into());
    }
    let root = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .context("Set HOME or AI_SESSION_HUB_CONFIG")?;
    Ok(root.join("ai-session-hub/config.toml"))
}
pub fn load() -> Result<Config> {
    let p = path()?;
    if !p.exists() {
        return Ok(Config::default());
    }
    parse(&fs::read_to_string(&p).with_context(|| format!("Cannot read {}", p.display()))?)
}
fn parse(text: &str) -> Result<Config> {
    let c: Config = toml::from_str(text).context("Invalid configuration")?;
    if c.refresh_seconds == 0
        || c.metadata_seconds == 0
        || c.content_seconds == 0
        || c.activity_seconds == 0
    {
        bail!(
            "refresh_seconds, metadata_seconds, content_seconds and activity_seconds must be positive"
        );
    }
    crate::theme::get(&c.theme)?;
    Ok(c)
}
pub fn setup() -> Result<()> {
    let p = path()?;
    if p.exists() {
        println!("Preserved {}", p.display());
        return Ok(());
    }
    fs::create_dir_all(
        p.parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new(".")),
    )?;
    use std::io::Write;
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&p)?;
    f.write_all(include_bytes!("../config.example.toml"))?;
    println!("Created {}", p.display());
    Ok(())
}
pub fn set_theme(name: &str) -> Result<()> {
    save_theme(name)?;
    println!("Theme: {name}");
    Ok(())
}
pub fn save_theme(name: &str) -> Result<()> {
    crate::theme::get(name)?;
    let p = path()?;
    let text = if p.exists() {
        fs::read_to_string(&p)?
    } else {
        include_str!("../config.example.toml").into()
    };
    let mut doc = text.parse::<toml_edit::DocumentMut>()?;
    doc["theme"] = toml_edit::value(name);
    parse(&doc.to_string())?;
    fs::create_dir_all(
        p.parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new(".")),
    )?;
    let temp = p.with_file_name(format!(".ai-session-hub-config-{}.tmp", std::process::id()));
    let result = (|| -> Result<()> {
        fs::write(&temp, doc.to_string())?;
        fs::rename(&temp, &p)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_zero_and_unknown_theme_preserves_defaults() {
        assert!(parse("refresh_seconds=0").is_err());
        assert!(parse("activity_seconds=0").is_err());
        let legacy = parse("content_seconds=3").unwrap();
        assert_eq!(legacy.content_seconds, 3);
        assert_eq!(legacy.activity_seconds, 10);
        assert!(parse("theme='unknown'").is_err());
        assert_eq!(parse("").unwrap().refresh_seconds, 2);
    }
}
