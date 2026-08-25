use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, Context, Result};
use serde_json::Value as Json;

use crate::paths;
use crate::profile::LoadedProfile;

pub struct Runner {
    pub dry_run: bool,
}

impl Runner {
    pub fn new(dry_run: bool) -> Self {
        Self { dry_run }
    }

    pub fn step(&self, msg: impl AsRef<str>) {
        let prefix = if self.dry_run { "would" } else { "" };
        let msg = msg.as_ref();
        if prefix.is_empty() {
            println!("  {msg}");
        } else {
            println!("  {prefix} {msg}");
        }
    }

    fn run(&self, program: &str, args: &[&str]) -> Result<()> {
        self.step(format!("run {program} {}", args.join(" ")));
        if self.dry_run {
            return Ok(());
        }
        let status = Command::new(program)
            .args(args)
            .status()
            .with_context(|| format!("spawning {program}"))?;
        if !status.success() {
            return Err(anyhow!("{program} exited with {status}"));
        }
        Ok(())
    }

    fn write(&self, path: &Path, contents: &str) -> Result<()> {
        if let Ok(existing) = std::fs::read_to_string(path) {
            if existing == contents {
                return Ok(());
            }
        }
        self.step(format!("write {}", paths::tilde(path)));
        if self.dry_run {
            return Ok(());
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        std::fs::write(path, contents).with_context(|| format!("writing {}", path.display()))
    }
}

/// Apply a theme's profile. Idempotent: running it twice changes nothing the
/// second time.
pub fn apply_profile(loaded: &LoadedProfile, runner: &Runner) -> Result<()> {
    println!(
        "omatheme: profile for {} ({}{})",
        loaded.theme,
        paths::tilde(&loaded.dir),
        if loaded.present {
            ""
        } else {
            ", no profile.toml — restoring defaults"
        }
    );

    if let Some(wallpaper) = &loaded.profile.wallpaper {
        let image = loaded.asset(&wallpaper.default)?;
        let current = paths::current_state()?.join("background");
        let already = std::fs::read_link(&current)
            .ok()
            .and_then(|target| target.canonicalize().ok())
            .map(|target| target == image)
            .unwrap_or(false);
        if !already {
            runner.run(
                "omarchy-theme-bg-set",
                &[image.to_str().ok_or_else(|| anyhow!("non-utf8 path"))?],
            )?;
        }
    }

    if let Some(font) = &loaded.profile.font {
        let current = Command::new("omarchy-font-current").output().ok();
        let current = current
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default();
        if current != font.name {
            runner.run("omarchy-font-set", &[&font.name])?;
        }
    }

    apply_shell(loaded, runner)?;
    apply_menu(loaded, runner)?;

    // No shell restart anywhere in here: the shell watches
    // ~/.config/omarchy/plugins/ with inotify, shell.json and the menu
    // extension with FileView.watchChanges. Everything lands live.
    crate::lock::apply(loaded, runner)?;
    crate::launcher::apply(loaded, runner)?;

    if let Some(commands) = &loaded.profile.commands {
        for command in &commands.post {
            runner.step(format!("run (post) {command}"));
            if runner.dry_run {
                continue;
            }
            let status = Command::new("bash")
                .arg("-lc")
                .arg(command)
                .env("OMATHEME_THEME", &loaded.theme)
                .env("OMATHEME_THEME_DIR", &loaded.dir)
                .status()
                .with_context(|| format!("running post command: {command}"))?;
            if !status.success() {
                eprintln!("omatheme: post command failed ({status}): {command}");
            }
        }
    }

    Ok(())
}

/// Top-level `shell.json` keys the running shell owns. They are never restored
/// from the baseline — they reflect live plugin state, not theme intent.
const RUNTIME_OWNED_KEYS: &[&str] = &["plugins", "disabledPlugins"];

/// `shell.json` is patched from an untouched baseline, so switching themes
/// never accumulates the previous theme's overrides.
fn apply_shell(loaded: &LoadedProfile, runner: &Runner) -> Result<()> {
    let config = paths::omarchy_config()?;
    let live = config.join("shell.json");
    let base = config.join("shell.base.json");

    if !live.is_file() {
        return Ok(());
    }

    let baseline = if base.is_file() {
        std::fs::read_to_string(&base).with_context(|| format!("reading {}", base.display()))?
    } else {
        let contents = std::fs::read_to_string(&live)
            .with_context(|| format!("reading {}", live.display()))?;
        runner.step(format!(
            "seed baseline {} from {}",
            paths::tilde(&base),
            paths::tilde(&live)
        ));
        if !runner.dry_run {
            std::fs::write(&base, &contents)
                .with_context(|| format!("writing {}", base.display()))?;
        }
        contents
    };

    let mut doc: Json =
        serde_json::from_str(&baseline).with_context(|| format!("parsing {}", base.display()))?;

    // The shell writes plugin enablement into shell.json itself
    // (`omarchy plugin enable` sets `plugins[]` and `disabledPlugins[]`).
    // Restoring those from the baseline would silently disable the lock clone
    // this run just installed, so carry the live values over.
    if let Ok(live_doc) = std::fs::read_to_string(&live)
        .map_err(anyhow::Error::from)
        .and_then(|raw| Ok(serde_json::from_str::<Json>(&raw)?))
    {
        for key in RUNTIME_OWNED_KEYS {
            match live_doc.get(key) {
                Some(value) => {
                    if let Some(object) = doc.as_object_mut() {
                        object.insert((*key).to_string(), value.clone());
                    }
                }
                None => {
                    if let Some(object) = doc.as_object_mut() {
                        object.remove(*key);
                    }
                }
            }
        }
    }

    if let Some(shell) = &loaded.profile.shell {
        for (dotted, value) in &shell.patch {
            set_dotted(&mut doc, dotted, toml_to_json(value)?)?;
        }
    }

    // Compare the parsed documents, not the bytes: reformatting a file the
    // user hand-edited would be a change they never asked for.
    if let Ok(current) = std::fs::read_to_string(&live) {
        if let Ok(current) = serde_json::from_str::<Json>(&current) {
            if current == doc {
                return Ok(());
            }
        }
    }

    let mut rendered = serde_json::to_string_pretty(&doc)?;
    rendered.push('\n');
    runner.write(&live, &rendered)
}

/// The launcher/menu extension follows the same baseline rule as `shell.json`.
fn apply_menu(loaded: &LoadedProfile, runner: &Runner) -> Result<()> {
    let live = paths::omarchy_config()?
        .join("extensions")
        .join("omarchy-menu.jsonc");
    let base = live.with_file_name("omarchy-menu.base.jsonc");

    let source: PathBuf = match &loaded.profile.menu {
        Some(menu) => loaded.asset(&menu.extension)?,
        None => {
            // No menu in this profile: restore the baseline if we ever moved it.
            if base.is_file() {
                base.clone()
            } else {
                return Ok(());
            }
        }
    };

    if !base.is_file() && live.is_file() {
        let contents = std::fs::read_to_string(&live)
            .with_context(|| format!("reading {}", live.display()))?;
        runner.step(format!("seed baseline {}", paths::tilde(&base)));
        if !runner.dry_run {
            std::fs::write(&base, contents)
                .with_context(|| format!("writing {}", base.display()))?;
        }
    }

    let contents = std::fs::read_to_string(&source)
        .with_context(|| format!("reading {}", source.display()))?;
    runner.write(&live, &contents)
}

fn toml_to_json(value: &toml::Value) -> Result<Json> {
    let json = serde_json::to_value(value)?;
    Ok(json)
}

/// Set `a.b.c` inside a JSON document, creating intermediate objects.
fn set_dotted(doc: &mut Json, dotted: &str, value: Json) -> Result<()> {
    let mut cursor = doc;
    let parts: Vec<&str> = dotted.split('.').collect();
    let (last, parents) = parts
        .split_last()
        .ok_or_else(|| anyhow!("empty key in [shell].patch"))?;

    for part in parents {
        if !cursor.is_object() {
            return Err(anyhow!("{dotted}: {part} is not an object"));
        }
        cursor = cursor
            .as_object_mut()
            .expect("checked above")
            .entry((*part).to_string())
            .or_insert_with(|| Json::Object(Default::default()));
    }

    let object = cursor
        .as_object_mut()
        .ok_or_else(|| anyhow!("{dotted}: parent is not an object"))?;
    object.insert((*last).to_string(), value);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::set_dotted;
    use serde_json::json;

    #[test]
    fn sets_nested_key() {
        let mut doc = json!({"bar": {"position": "top"}});
        set_dotted(&mut doc, "bar.position", json!("left")).unwrap();
        assert_eq!(doc["bar"]["position"], json!("left"));
    }

    #[test]
    fn creates_missing_parents() {
        let mut doc = json!({});
        set_dotted(&mut doc, "idle.lock", json!(300)).unwrap();
        assert_eq!(doc["idle"]["lock"], json!(300));
    }

    #[test]
    fn refuses_to_descend_into_a_scalar() {
        let mut doc = json!({"bar": 1});
        assert!(set_dotted(&mut doc, "bar.position", json!("left")).is_err());
    }
}
