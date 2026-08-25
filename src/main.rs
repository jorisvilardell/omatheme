mod apply;
mod doctor;
mod paths;
mod profile;
mod scaffold;

use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use crate::apply::Runner;
use crate::profile::LoadedProfile;

/// Theme profiles for Omarchy.
///
/// Omarchy already renders colours into every app on `omarchy theme set`.
/// omatheme carries the rest of a theme — wallpaper, font, shell layout, menu,
/// arbitrary per-theme commands — declared in a `profile.toml` inside the theme
/// directory.
#[derive(Parser)]
#[command(name = "omatheme", version, about, long_about = None)]
struct Cli {
    /// Print what would change without touching anything.
    #[arg(long, global = true)]
    dry_run: bool,

    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Switch to a theme, then apply its profile.
    Apply {
        /// Theme name or slug ("Gojo Latte" and "gojo-latte" both work).
        theme: String,
    },
    /// Apply the profile of the current theme without switching.
    Sync,
    /// Entry point for the Omarchy `theme-set` hook (takes the theme as $1).
    Hook {
        /// Theme name passed by omarchy-hook.
        theme: Option<String>,
    },
    /// Install this binary as the Omarchy `theme-set` hook.
    InstallHook,
    /// Create a theme skeleton.
    New {
        /// Theme name.
        name: String,
        /// Wallpaper to seed the theme with.
        #[arg(long, value_name = "IMAGE")]
        from: Option<PathBuf>,
    },
    /// Report colour literals that will not follow a theme switch.
    Doctor,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("omatheme: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let runner = Runner::new(cli.dry_run);

    match cli.command {
        Cmd::Apply { theme } => {
            let slug = paths::slugify(&theme);
            // The theme switch fires the theme-set hook, which applies the
            // profile when the hook is installed. Applying it again right
            // after is harmless — every step is idempotent — and keeps the
            // command working on a machine without the hook.
            if cli.dry_run {
                println!("  would run omarchy-theme-set {slug}");
            } else {
                let status = Command::new("omarchy-theme-set")
                    .arg(&slug)
                    .status()
                    .context("running omarchy-theme-set")?;
                if !status.success() {
                    anyhow::bail!("omarchy-theme-set exited with {status}");
                }
            }
            apply_named(&slug, &runner)
        }
        Cmd::Sync => {
            let current = paths::current_theme_name()?;
            apply_named(&current, &runner)
        }
        Cmd::Hook { theme } => {
            let theme = match theme {
                Some(theme) => theme,
                None => paths::current_theme_name()?,
            };
            apply_named(&theme, &runner)
        }
        Cmd::InstallHook => install_hook(&runner),
        Cmd::New { name, from } => {
            scaffold::new_theme(&name, from.as_deref(), cli.dry_run)?;
            Ok(())
        }
        Cmd::Doctor => {
            let clean = doctor::run()?;
            if !clean {
                std::process::exit(1);
            }
            Ok(())
        }
    }
}

fn apply_named(theme: &str, runner: &Runner) -> Result<()> {
    let dir = paths::theme_dir(theme)?;
    let loaded = LoadedProfile::load(&paths::slugify(theme), &dir)?;
    apply::apply_profile(&loaded, runner)
}

/// The hook is a tiny shell stub calling this binary, so upgrading omatheme
/// never requires reinstalling the hook.
fn install_hook(runner: &Runner) -> Result<()> {
    let exe = std::env::current_exe().context("locating the omatheme binary")?;
    let hook_dir = paths::omarchy_config()?.join("hooks/theme-set.d");
    let hook = hook_dir.join("omatheme.hook");
    let body = format!(
        "#!/bin/bash\n# Installed by `omatheme install-hook`.\n# $1 is the theme slug passed by omarchy-hook.\nexec {} hook \"$1\"\n",
        exe.display()
    );

    println!("omatheme: installing {}", paths::tilde(&hook));
    if runner.dry_run {
        println!("  would write {}", paths::tilde(&hook));
        return Ok(());
    }

    std::fs::create_dir_all(&hook_dir)
        .with_context(|| format!("creating {}", hook_dir.display()))?;
    std::fs::write(&hook, body).with_context(|| format!("writing {}", hook.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755))?;
    }

    println!("  every `omarchy theme set` now applies the theme's profile.toml");
    Ok(())
}
