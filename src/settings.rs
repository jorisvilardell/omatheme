use std::collections::BTreeMap;

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::paths;

/// Machine-level opt-outs, `~/.config/omatheme/config.toml`.
///
/// A theme bundles what its author thinks belongs together; the person running
/// it may only want some of it. Someone who likes a theme's colours but not its
/// launcher writes:
///
/// ```toml
/// [themes.spiderverse]
/// launcher = false
/// lock = false
/// ```
///
/// or turns a part off everywhere with a top-level `launcher = false`.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    #[serde(flatten)]
    global: Toggles,
    /// Per-theme overrides, keyed by theme slug. They win over the globals.
    #[serde(default)]
    themes: BTreeMap<String, Toggles>,
}

#[derive(Debug, Default, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Toggles {
    wallpaper: Option<bool>,
    font: Option<bool>,
    shell: Option<bool>,
    menu: Option<bool>,
    lock: Option<bool>,
    launcher: Option<bool>,
    commands: Option<bool>,
}

/// One part of a profile, so it can be switched off by name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    Wallpaper,
    Font,
    Shell,
    Menu,
    Lock,
    Launcher,
    Commands,
}

impl Settings {
    pub fn load() -> Result<Self> {
        let path = paths::config_home()?.join("omatheme/config.toml");
        if !path.is_file() {
            return Ok(Self::default());
        }
        let raw = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        toml::from_str(&raw).with_context(|| format!("parsing {}", path.display()))
    }

    /// Parts default to on: a profile does what it says unless told otherwise.
    pub fn enabled(&self, theme: &str, part: Part) -> bool {
        let per_theme = self.themes.get(theme).and_then(|t| t.get(part));
        per_theme.or_else(|| self.global.get(part)).unwrap_or(true)
    }
}

impl Toggles {
    fn get(&self, part: Part) -> Option<bool> {
        match part {
            Part::Wallpaper => self.wallpaper,
            Part::Font => self.font,
            Part::Shell => self.shell,
            Part::Menu => self.menu,
            Part::Lock => self.lock,
            Part::Launcher => self.launcher,
            Part::Commands => self.commands,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Part, Settings};

    fn parse(raw: &str) -> Settings {
        toml::from_str(raw).unwrap()
    }

    #[test]
    fn everything_is_on_by_default() {
        let settings = Settings::default();
        assert!(settings.enabled("spiderverse", Part::Lock));
    }

    #[test]
    fn per_theme_beats_global() {
        let settings = parse("launcher = false\n[themes.spiderverse]\nlauncher = true\n");
        assert!(settings.enabled("spiderverse", Part::Launcher));
        assert!(!settings.enabled("arcane", Part::Launcher));
    }

    #[test]
    fn unset_parts_stay_on() {
        let settings = parse("[themes.spiderverse]\nlock = false\n");
        assert!(!settings.enabled("spiderverse", Part::Lock));
        assert!(settings.enabled("spiderverse", Part::Launcher));
    }
}
