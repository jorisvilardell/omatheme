use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Command;

use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;

use crate::lock::copy_into;
use crate::paths;

/// Manifest a payload repository puts at its root, `omatheme.toml`.
///
/// A payload is the non-colour half of a look — a lock screen, a launcher —
/// living in its own repository so its author keeps maintaining it there. The
/// manifest says which paths hold what, so `omatheme install` can graft it onto
/// a theme without every repo inventing its own install script.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub payload: Payload,
    #[serde(default)]
    pub lock: Option<LockPayload>,
    #[serde(default)]
    pub launcher: Option<LauncherPayload>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Payload {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    /// Theme this payload was written for. Used as the default target, so a
    /// matching theme needs no second argument.
    #[serde(default)]
    pub theme: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockPayload {
    /// Directory of presentational QML, relative to the repository root.
    /// `Service.qml` must not be in it: omatheme reclones that from the running
    /// Omarchy on every theme switch.
    pub overlay: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LauncherPayload {
    pub command: String,
    #[serde(default)]
    pub quickshell: Option<QuickshellPayload>,
    #[serde(default)]
    pub bin: Option<String>,
    #[serde(default)]
    pub autostart: Option<bool>,
    #[serde(default)]
    pub verbs: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuickshellPayload {
    pub source: String,
    pub name: String,
}

/// `omatheme install <url> [--theme <slug>]`.
///
/// One entry point for both halves: a repository holding `colors.toml` is a
/// theme and goes to `omarchy theme install`; one holding `omatheme.toml` is a
/// payload and gets grafted onto a theme.
pub fn install(url: &str, theme: Option<&str>, dry_run: bool) -> Result<()> {
    let checkout = clone(url)?;
    let manifest_path = checkout.path().join("omatheme.toml");
    let is_theme = checkout.path().join("colors.toml").is_file();

    if !manifest_path.is_file() {
        if !is_theme {
            bail!("{url} is neither a theme (no colors.toml) nor a payload (no omatheme.toml)");
        }
        println!("omatheme: {url} is a theme — handing over to omarchy");
        if dry_run {
            println!("  would run omarchy-theme-install {url}");
            return Ok(());
        }
        let status = Command::new("omarchy-theme-install")
            .arg(url)
            .status()
            .context("running omarchy-theme-install")?;
        if !status.success() {
            bail!("omarchy-theme-install exited with {status}");
        }
        return Ok(());
    }

    let raw = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("reading {}", manifest_path.display()))?;
    let manifest: Manifest =
        toml::from_str(&raw).with_context(|| format!("parsing {}", manifest_path.display()))?;

    let target = theme
        .map(str::to_string)
        .or_else(|| manifest.payload.theme.clone())
        .ok_or_else(|| {
            anyhow!(
                "{} does not name a theme — pass the one to link it to: omatheme install {url} --theme <theme>",
                manifest.payload.name
            )
        })?;
    let slug = paths::slugify(&target);
    let theme_dir = paths::theme_dir(&slug).with_context(|| {
        format!("resolving theme {slug} — install it first with omatheme install <theme-url>")
    })?;

    println!(
        "omatheme: {} -> {}",
        manifest.payload.name,
        paths::tilde(&theme_dir)
    );
    if let Some(description) = &manifest.payload.description {
        println!("  {description}");
    }

    let mut profile = String::from(
        "# Written by `omatheme install`. Edit freely — it is not regenerated\n# unless you install the payload again.\n",
    );

    if let Some(lock) = &manifest.lock {
        let source = checkout.path().join(&lock.overlay);
        if !source.is_dir() {
            bail!("{} is not a directory in the payload", lock.overlay);
        }
        if source.join("Service.qml").is_file() {
            bail!(
                "{}/Service.qml is present — a payload must ship the presentational files only, \
                 the service comes from the running Omarchy",
                lock.overlay
            );
        }
        println!("  lock screen -> lock/");
        if !dry_run {
            copy_into(&source, &theme_dir.join("lock"))?;
        }
        profile.push_str("\n[lock]\noverlay = \"lock\"\n");
    }

    if let Some(launcher) = &manifest.launcher {
        profile.push_str("\n[launcher]\n");
        profile.push_str(&format!("command = {:?}\n", launcher.command));

        if let Some(app) = &launcher.quickshell {
            let source = checkout.path().join(&app.source);
            if !source.is_dir() {
                bail!("{} is not a directory in the payload", app.source);
            }
            println!("  launcher app -> launcher/quickshell/");
            if !dry_run {
                copy_into(&source, &theme_dir.join("launcher/quickshell"))?;
            }
            profile.push_str(&format!(
                "quickshell = {{ source = \"launcher/quickshell\", name = {:?} }}\n",
                app.name
            ));
        }

        if let Some(bin) = &launcher.bin {
            let source = checkout.path().join(bin);
            let name = source
                .file_name()
                .ok_or_else(|| anyhow!("{bin} has no file name"))?
                .to_string_lossy()
                .to_string();
            println!("  launcher script -> launcher/bin/{name}");
            if !dry_run {
                let target = theme_dir.join("launcher/bin");
                std::fs::create_dir_all(&target)?;
                std::fs::copy(&source, target.join(&name))
                    .with_context(|| format!("copying {}", source.display()))?;
            }
            profile.push_str(&format!("bin = \"launcher/bin/{name}\"\n"));
        }

        if launcher.autostart.unwrap_or(false) {
            profile.push_str("autostart = true\n");
        }
        if !launcher.verbs.is_empty() {
            let pairs: Vec<String> = launcher
                .verbs
                .iter()
                .map(|(verb, mapped)| format!("{verb} = {mapped:?}"))
                .collect();
            profile.push_str(&format!("verbs = {{ {} }}\n", pairs.join(", ")));
        }
    }

    let profile_path = theme_dir.join("profile.toml");
    if profile_path.is_file() {
        // A theme may already pin its wallpaper or patch the shell; do not
        // clobber the author's file with a generated one.
        let generated = profile_path.with_file_name("profile.omatheme.toml");
        println!(
            "  {} exists — writing {} instead, merge what you want",
            paths::tilde(&profile_path),
            paths::tilde(&generated)
        );
        if !dry_run {
            std::fs::write(&generated, &profile)?;
        }
    } else {
        println!("  profile -> {}", paths::tilde(&profile_path));
        if !dry_run {
            std::fs::write(&profile_path, &profile)?;
        }
    }

    if !dry_run {
        println!("\nApply it with: omatheme apply {slug}");
    }
    Ok(())
}

/// A shallow clone in a temporary directory, removed when it drops.
struct Checkout(PathBuf);

impl Checkout {
    fn path(&self) -> PathBuf {
        self.0.join("repo")
    }
}

impl Drop for Checkout {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn clone(url: &str) -> Result<Checkout> {
    let base = std::env::temp_dir().join(format!("omatheme-install-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).with_context(|| format!("creating {}", base.display()))?;

    let status = Command::new("git")
        .args(["clone", "--depth", "1", "--quiet", url])
        .arg(&base.join("repo"))
        .status()
        .context("running git clone")?;
    if !status.success() {
        bail!("git clone {url} exited with {status}");
    }
    Ok(Checkout(base))
}
