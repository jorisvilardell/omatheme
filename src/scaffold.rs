use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};

use crate::paths;

const COLORS_TEMPLATE: &str = r##"# Palette de <NAME>.
# Omarchy dérive automatiquement brown, dark_background, darker_background et
# les bright_* si on les omet.

mode = "dark"

accent = "#7aa2f7"
selection = "#292e42"
muted = "#414868"

background = "#1a1b26"
lighter_background = "#24283b"

foreground = "#a9b1d6"
light_foreground = "#b4bee6"
dark_foreground = "#565f89"
bright_foreground = "#c0caf5"

red = "#f7768e"
yellow = "#e0af68"
green = "#9ece6a"
cyan = "#449dab"
blue = "#7aa2f7"
magenta = "#ad8ee6"
"##;

/// Create a theme skeleton Omarchy can already apply: colours, the wallpaper,
/// a preview and a profile pinning that wallpaper.
pub fn new_theme(name: &str, wallpaper: Option<&Path>, dry_run: bool) -> Result<PathBuf> {
    let slug = paths::slugify(name);
    let dir = paths::omarchy_config()?.join("themes").join(&slug);
    if dir.exists() {
        return Err(anyhow!("{} already exists", paths::tilde(&dir)));
    }

    let backgrounds = dir.join("backgrounds");
    let mut wallpaper_rel = None;

    if dry_run {
        println!("would create {}", paths::tilde(&dir));
        println!("would write  {}/colors.toml", paths::tilde(&dir));
        if let Some(source) = wallpaper {
            println!("would copy   {} into backgrounds/", source.display());
        }
        return Ok(dir);
    }

    std::fs::create_dir_all(&backgrounds)
        .with_context(|| format!("creating {}", backgrounds.display()))?;

    std::fs::write(
        dir.join("colors.toml"),
        COLORS_TEMPLATE.replace("<NAME>", name),
    )?;

    if let Some(source) = wallpaper {
        let file_name = source
            .file_name()
            .ok_or_else(|| anyhow!("{} has no file name", source.display()))?;
        let target = backgrounds.join(file_name);
        std::fs::copy(source, &target).with_context(|| format!("copying {}", source.display()))?;
        wallpaper_rel = Some(format!("backgrounds/{}", file_name.to_string_lossy()));

        // Best effort: a preview makes the theme show up properly in the
        // switcher. Missing ImageMagick is not an error.
        let _ = std::process::Command::new("magick")
            .arg(format!("{}[0]", target.display()))
            .args(["-resize", "640x360^", "-gravity", "center"])
            .args(["-extent", "640x360", "-strip"])
            .arg(dir.join("preview.png"))
            .status();
    }

    let profile = match &wallpaper_rel {
        Some(rel) => format!("[wallpaper]\ndefault = \"{rel}\"\n"),
        None => "# [wallpaper]\n# default = \"backgrounds/example.jpg\"\n".to_string(),
    };
    std::fs::write(dir.join("profile.toml"), profile)?;

    println!("created {}", paths::tilde(&dir));
    println!("apply it with: omatheme apply {slug}");
    Ok(dir)
}
