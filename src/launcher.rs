use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Command;

use anyhow::{anyhow, Context, Result};

use crate::apply::Runner;
use crate::lock::copy_into;
use crate::paths;
use crate::profile::LoadedProfile;

/// What Omarchy binds to SUPER + SPACE out of the box.
const DEFAULT_COMMAND: &str = "omarchy-menu";

/// Install the theme's launcher and remember which command is current, so the
/// keybinding can stay a single stable entry point.
pub fn apply(loaded: &LoadedProfile, runner: &Runner, enabled: bool) -> Result<()> {
    let previous = current_command()?;
    let declared = loaded.profile.launcher.as_ref().filter(|_| enabled);
    let command = match declared {
        Some(launcher) => launcher.command.clone(),
        None => DEFAULT_COMMAND.to_string(),
    };

    if let Some(launcher) = declared {
        if let Some(app) = &launcher.quickshell {
            let source = loaded.asset(&app.source)?;
            let target = paths::config_home()?.join("quickshell").join(&app.name);
            if runner.dry_run {
                runner.step(format!("install quickshell app {}", paths::tilde(&target)));
            } else if copy_into(&source, &target)? {
                runner.step(format!(
                    "installed quickshell app {}",
                    paths::tilde(&target)
                ));
            }
        }

        if let Some(bin) = &launcher.bin {
            let source = loaded.asset(bin)?;
            let name = source
                .file_name()
                .ok_or_else(|| anyhow!("{} has no file name", source.display()))?;
            let target = paths::home()?.join(".local/bin").join(name);
            if std::fs::read(&source).ok() != std::fs::read(&target).ok() {
                runner.step(format!("install {}", paths::tilde(&target)));
            }
            if !runner.dry_run {
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::copy(&source, &target)
                    .with_context(|| format!("copying {}", source.display()))?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755))?;
                }
            }
        }
    }

    if previous.as_deref() == Some(command.as_str()) {
        return Ok(());
    }

    // A launcher from the previous theme may be a long-lived Quickshell
    // process; ask it to stop before switching. Best effort: not every
    // launcher understands `stop`.
    if let Some(previous) = previous {
        if previous != DEFAULT_COMMAND {
            runner.step(format!("stop the previous launcher ({previous})"));
            if !runner.dry_run {
                let _ = Command::new(&previous).arg("stop").status();
            }
        }
    }

    runner.step(format!("launcher command -> {command}"));
    if !runner.dry_run {
        let path = state_file()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, format!("{command}\n"))
            .with_context(|| format!("writing {}", path.display()))?;

        let verbs = path.with_file_name("launcher.verbs");
        let mapping: String = declared
            .map(|launcher| {
                launcher
                    .verbs
                    .iter()
                    .map(|(verb, mapped)| format!("{verb}\t{mapped}\n"))
                    .collect()
            })
            .unwrap_or_default();
        std::fs::write(&verbs, mapping).with_context(|| format!("writing {}", verbs.display()))?;
    }

    let autostart = declared
        .and_then(|launcher| launcher.autostart)
        .unwrap_or(false);
    if autostart && command != DEFAULT_COMMAND {
        runner.step(format!("start {command}"));
        if !runner.dry_run {
            let _ = Command::new(&command).arg("start").status();
        }
    }

    Ok(())
}

/// Run the current theme's launcher. `omatheme launcher menu` is what the
/// keybinding calls, so switching themes never means rebinding a key.
///
/// Launchers do not agree on verbs — Omarchy's menu says `toggle` and
/// `toggle apps`, another may say `menu` and `toggle`. The keybinding speaks
/// one vocabulary and the profile maps it onto whatever the launcher accepts.
pub fn dispatch(args: &[String]) -> Result<()> {
    let command = current_command()?.unwrap_or_else(|| DEFAULT_COMMAND.to_string());
    let verbs = current_verbs()?;

    let mut translated: Vec<String> = Vec::new();
    if let Some((first, rest)) = args.split_first() {
        match verbs.get(first.as_str()) {
            Some(mapped) => translated.extend(mapped.split_whitespace().map(String::from)),
            None => translated.push(first.clone()),
        }
        translated.extend(rest.iter().cloned());
    }

    let status = Command::new(&command)
        .args(&translated)
        .status()
        .with_context(|| format!("running {command}"))?;
    std::process::exit(status.code().unwrap_or(1));
}

/// Verb map of the launcher in use. Omarchy's menu is the fallback, so its
/// mapping is built in: the root menu is `toggle`, the app list `toggle apps`.
fn current_verbs() -> Result<BTreeMap<String, String>> {
    let path = state_file()?.with_file_name("launcher.verbs");
    let raw = std::fs::read_to_string(&path).unwrap_or_default();
    let mut verbs: BTreeMap<String, String> = BTreeMap::new();

    for line in raw.lines() {
        if let Some((verb, mapped)) = line.split_once('\t') {
            verbs.insert(verb.to_string(), mapped.to_string());
        }
    }

    if verbs.is_empty()
        && current_command()?.as_deref().unwrap_or(DEFAULT_COMMAND) == DEFAULT_COMMAND
    {
        verbs.insert("menu".into(), "toggle".into());
        verbs.insert("apps".into(), "toggle apps".into());
    }
    Ok(verbs)
}

fn current_command() -> Result<Option<String>> {
    let path = state_file()?;
    match std::fs::read_to_string(&path) {
        Ok(command) => {
            let command = command.trim().to_string();
            Ok(if command.is_empty() {
                None
            } else {
                Some(command)
            })
        }
        Err(_) => Ok(None),
    }
}

fn state_file() -> Result<PathBuf> {
    Ok(paths::home()?.join(".local/state/omatheme/launcher"))
}
