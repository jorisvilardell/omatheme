use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;

/// Everything a theme carries beyond colours. Lives as `profile.toml` inside
/// the theme directory; Omarchy itself ignores the file, so a theme with a
/// profile stays a perfectly valid stock theme.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    #[serde(default)]
    pub wallpaper: Option<Wallpaper>,
    #[serde(default)]
    pub font: Option<Font>,
    #[serde(default)]
    pub shell: Option<Shell>,
    #[serde(default)]
    pub menu: Option<Menu>,
    #[serde(default)]
    pub lock: Option<Lock>,
    #[serde(default)]
    pub launcher: Option<Launcher>,
    #[serde(default)]
    pub commands: Option<Commands>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Wallpaper {
    /// Path relative to the theme directory, e.g. `backgrounds/1-glitch.jpg`.
    /// Pins the wallpaper instead of letting Omarchy cycle alphabetically.
    pub default: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Font {
    pub name: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Shell {
    /// Dotted paths into `shell.json`, e.g. `"bar.position" = "left"`.
    /// Applied on top of the untouched base, never on the previous theme's
    /// result.
    #[serde(default)]
    pub patch: BTreeMap<String, toml::Value>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Menu {
    /// Path relative to the theme directory of a replacement
    /// `omarchy-menu.jsonc`.
    pub extension: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lock {
    /// Directory, relative to the theme, whose files are overlaid on a fresh
    /// clone of the `omarchy.lock` plugin. Ship only the presentational files
    /// (`LockView.qml` and what it pulls in) — never `Service.qml`, which
    /// carries the PAM and session-lock logic.
    pub overlay: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Launcher {
    /// Command `omatheme launcher <verb>` forwards to. Defaults to
    /// `omarchy-menu` when no theme declares one.
    pub command: String,
    /// Quickshell app to install under `~/.config/quickshell/<name>`.
    #[serde(default)]
    pub quickshell: Option<QuickshellApp>,
    /// Helper script to install into `~/.local/bin`, relative to the theme.
    #[serde(default)]
    pub bin: Option<String>,
    /// Run `<command> start` right after applying the theme.
    #[serde(default)]
    pub autostart: Option<bool>,
    /// Maps the verbs a keybinding uses onto the ones this launcher speaks,
    /// e.g. `menu = "menu"`, `apps = "toggle"`. Verbs with no entry are passed
    /// through unchanged.
    #[serde(default)]
    pub verbs: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuickshellApp {
    /// Directory relative to the theme, containing `shell.qml`.
    pub source: String,
    /// Name under `~/.config/quickshell/`, the one `qs -c <name>` takes.
    pub name: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Commands {
    /// Escape hatch: shell commands run after everything else, with
    /// `OMATHEME_THEME` and `OMATHEME_THEME_DIR` in the environment. Covers
    /// what has no first-class field yet (quickshell plugins, hyprlock, ...).
    #[serde(default)]
    pub post: Vec<String>,
}

pub struct LoadedProfile {
    pub theme: String,
    pub dir: PathBuf,
    pub profile: Profile,
    /// False when the theme ships no `profile.toml`; the defaults still apply
    /// so a themed machine reverts cleanly.
    pub present: bool,
}

impl LoadedProfile {
    pub fn load(theme: &str, dir: &Path) -> Result<Self> {
        let path = dir.join("profile.toml");
        let (profile, present) = if path.is_file() {
            let raw = std::fs::read_to_string(&path)
                .with_context(|| format!("reading {}", path.display()))?;
            let parsed: Profile =
                toml::from_str(&raw).with_context(|| format!("parsing {}", path.display()))?;
            (parsed, true)
        } else {
            (Profile::default(), false)
        };

        Ok(Self {
            theme: theme.to_string(),
            dir: dir.to_path_buf(),
            profile,
            present,
        })
    }

    /// Resolve a theme-relative asset, refusing anything that escapes the
    /// theme directory.
    pub fn asset(&self, rel: &str) -> Result<PathBuf> {
        let candidate = self.dir.join(rel);
        let theme_dir = self
            .dir
            .canonicalize()
            .with_context(|| format!("resolving {}", self.dir.display()))?;
        let resolved = candidate
            .canonicalize()
            .with_context(|| format!("resolving {}", candidate.display()))?;
        if !resolved.starts_with(&theme_dir) {
            return Err(anyhow!(
                "{rel} escapes the theme directory ({})",
                resolved.display()
            ));
        }
        Ok(resolved)
    }
}
