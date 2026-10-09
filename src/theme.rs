use anyhow::{Result, bail};
use ratatui::style::Color;
use serde::Deserialize;

pub const NAMES: [&str; 5] = ["classic", "forest", "sunset", "ocean", "midnight"];
#[derive(Debug, Deserialize)]
pub struct Theme {
    pub name: String,
    #[serde(deserialize_with = "color")]
    pub list_border: Color,
    #[serde(deserialize_with = "color")]
    pub detail_border: Color,
    #[serde(deserialize_with = "color")]
    pub label: Color,
    #[serde(deserialize_with = "color")]
    pub project: Color,
    #[serde(deserialize_with = "color")]
    pub task: Color,
    #[serde(deserialize_with = "color")]
    pub selected_bg: Color,
    #[serde(deserialize_with = "color")]
    pub selected_fg: Color,
    #[serde(deserialize_with = "color")]
    pub status: Color,
    #[serde(deserialize_with = "color")]
    pub info: Color,
    #[serde(deserialize_with = "color")]
    pub error: Color,
}
fn color<'de, D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Color, D::Error> {
    let s = String::deserialize(d)?;
    s.parse().map_err(serde::de::Error::custom)
}
pub fn get(name: &str) -> Result<Theme> {
    let text = match name {
        "classic" => include_str!("../themes/classic.toml"),
        "forest" => include_str!("../themes/forest.toml"),
        "sunset" => include_str!("../themes/sunset.toml"),
        "ocean" => include_str!("../themes/ocean.toml"),
        "midnight" => include_str!("../themes/midnight.toml"),
        _ => bail!("Unknown theme: {name}; run sova themes"),
    };
    let theme: Theme = toml::from_str(text)?;
    if theme.name != name {
        bail!("Bundled theme name mismatch");
    }
    Ok(theme)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn palettes_differ() {
        for (i, a) in NAMES.iter().enumerate() {
            for b in &NAMES[i + 1..] {
                let a = get(a).unwrap();
                let b = get(b).unwrap();
                assert_ne!(
                    (a.list_border, a.detail_border, a.selected_bg),
                    (b.list_border, b.detail_border, b.selected_bg)
                );
            }
        }
    }
}
