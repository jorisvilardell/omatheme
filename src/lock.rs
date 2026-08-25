use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, Context, Result};

use crate::apply::Runner;
use crate::paths;
use crate::profile::LoadedProfile;

/// Marker dropped inside a plugin clone omatheme created. Only clones carrying
/// it are ever deleted — a clone the user made by hand is left alone.
const MARKER: &str = ".omatheme-managed";

/// The built-in plugin a themed lock screen clones from.
const SOURCE_ID: &str = "omarchy.lock";

/// On Quattro the lock screen is a Quickshell *service* plugin. `Service.qml`
/// holds the PAM and `ext-session-lock-v1` logic; `LockView.qml` and friends
/// are the purely visual layer. A theme overlays only the visual files, and the
/// clone is taken fresh from the installed Omarchy every time, so the security
/// logic is never frozen at the version a theme was authored against.
///
/// Returns true when the lock plugin changed. The shell reloads it by itself —
/// it watches ~/.config/omarchy/plugins/ with inotify.
pub fn apply(loaded: &LoadedProfile, runner: &Runner, enabled: bool) -> Result<bool> {
    let clone = clone_dir()?;
    let owner = owner_of(&clone);

    let Some(lock) = &loaded.profile.lock.as_ref().filter(|_| enabled) else {
        return remove_if_ours(&clone, owner.as_deref(), runner);
    };

    let overlay = loaded.asset(&lock.overlay)?;
    if !overlay.is_dir() {
        return Err(anyhow!("{} is not a directory", overlay.display()));
    }

    // The clone is already ours for this theme, but the theme's files may have
    // changed since — an author editing their overlay expects a re-apply to
    // pick it up. copy_into only writes what differs, so this stays cheap.
    if owner.as_deref() == Some(loaded.theme.as_str()) {
        let changed = copy_into(&overlay, &clone)?;
        if changed {
            runner.step(format!("refresh the lock overlay in {}", paths::tilde(&clone)));
        }
        return Ok(changed);
    }
    if clone.exists() && owner.is_none() {
        eprintln!(
            "omatheme: {} exists and was not created by omatheme — leaving the lock screen alone",
            paths::tilde(&clone)
        );
        return Ok(false);
    }

    runner.step(format!(
        "reclone omarchy.lock into {} and overlay {}",
        paths::tilde(&clone),
        paths::tilde(&overlay)
    ));
    if runner.dry_run {
        return Ok(true);
    }

    drop_clone(&clone)?;

    let status = Command::new("omarchy-plugin-clone")
        .arg(SOURCE_ID)
        .status()
        .context("running omarchy-plugin-clone")?;
    if !status.success() {
        return Err(anyhow!("omarchy-plugin-clone exited with {status}"));
    }

    copy_into(&overlay, &clone)?;
    std::fs::write(clone.join(MARKER), format!("{}\n", loaded.theme))
        .with_context(|| format!("writing {}", clone.join(MARKER).display()))?;

    Ok(true)
}

/// `omarchy plugin clone` always targets `<username>.<id>`.
fn clone_dir() -> Result<PathBuf> {
    let user = std::env::var("USER").unwrap_or_default();
    let user = if user.is_empty() {
        return Err(anyhow!("USER is not set"));
    } else {
        user
    };
    Ok(paths::omarchy_config()?
        .join("plugins")
        .join(format!("{user}.lock")))
}

/// Which theme, if any, owns this clone.
fn owner_of(clone: &Path) -> Option<String> {
    std::fs::read_to_string(clone.join(MARKER))
        .ok()
        .map(|owner| owner.trim().to_string())
}

fn remove_if_ours(clone: &Path, owner: Option<&str>, runner: &Runner) -> Result<bool> {
    if !clone.exists() || owner.is_none() {
        return Ok(false);
    }
    runner.step(format!(
        "restore the stock lock screen (remove {})",
        paths::tilde(clone)
    ));
    if runner.dry_run {
        return Ok(true);
    }
    drop_clone(clone)?;
    Ok(true)
}

/// Deleting the directory is not enough: enabling a clone pushes its
/// `clonedFrom` id into `disabledPlugins`, so the built-in lock screen stays
/// off until the clone is disabled through the shell. Disable first, then
/// delete, then rescan.
fn drop_clone(clone: &Path) -> Result<()> {
    if !clone.exists() {
        return Ok(());
    }
    let id = clone
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow!("bad plugin path {}", clone.display()))?;

    let _ = Command::new("omarchy-plugin-disable").arg(id).status();
    std::fs::remove_dir_all(clone).with_context(|| format!("removing {}", clone.display()))?;
    let _ = Command::new("omarchy-shell")
        .args(["shell", "rescanPlugins"])
        .status();

    // Disabling the clone is meant to restore its source, but the source stays
    // in `disabledPlugins` once the clone's directory is gone — which would
    // leave the session with no lock screen at all. Re-enable it explicitly.
    let _ = Command::new("omarchy-plugin-enable")
        .arg(SOURCE_ID)
        .status();
    Ok(())
}

/// Copy a directory's contents over an existing one, file by file. Identical
/// files are left untouched — this runs on every theme switch, and rewriting
/// them would wake the shell's inotify watcher for nothing. Returns whether
/// anything changed.
pub fn copy_into(source: &Path, target: &Path) -> Result<bool> {
    std::fs::create_dir_all(target).with_context(|| format!("creating {}", target.display()))?;
    let mut changed = false;

    for entry in std::fs::read_dir(source)
        .with_context(|| format!("reading {}", source.display()))?
        .flatten()
    {
        let from = entry.path();
        let to = target.join(entry.file_name());
        if from.is_dir() {
            changed |= copy_into(&from, &to)?;
            continue;
        }
        if std::fs::read(&from).ok() == std::fs::read(&to).ok() {
            continue;
        }
        std::fs::copy(&from, &to)
            .with_context(|| format!("copying {} to {}", from.display(), to.display()))?;
        changed = true;
    }
    Ok(changed)
}
