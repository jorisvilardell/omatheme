use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};

/// `$HOME`, the anchor for every other path.
pub fn home() -> Result<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("HOME is not set"))
}

/// `$XDG_CONFIG_HOME`, falling back to `~/.config`.
pub fn config_home() -> Result<PathBuf> {
    if let Some(dir) = std::env::var_os("XDG_CONFIG_HOME") {
        let dir = PathBuf::from(dir);
        if dir.is_absolute() {
            return Ok(dir);
        }
    }
    Ok(home()?.join(".config"))
}

pub fn omarchy_config() -> Result<PathBuf> {
    Ok(config_home()?.join("omarchy"))
}

/// Where Omarchy keeps the theme it resolved for the running session.
pub fn current_state() -> Result<PathBuf> {
    Ok(home()?.join(".local/state/omarchy/current"))
}

/// Root of the Omarchy package, overridable through `$OMARCHY_PATH`.
pub fn omarchy_share() -> PathBuf {
    std::env::var_os("OMARCHY_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/usr/share/omarchy"))
}

/// Normalize a theme name the way `omarchy-theme-set` does: drop `<...>` tags,
/// lowercase, spaces to dashes. "Gojo Latte" and "gojo-latte" both resolve.
pub fn slugify(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut in_tag = false;
    for ch in name.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if in_tag => {}
            ' ' | '\t' | '_' => out.push('-'),
            _ => out.extend(ch.to_lowercase()),
        }
    }
    out.trim_matches('-').to_string()
}

/// User themes win over the ones shipped by the package, same as
/// `omarchy-theme-dir`.
pub fn theme_dir(name: &str) -> Result<PathBuf> {
    let slug = slugify(name);
    let user = omarchy_config()?.join("themes").join(&slug);
    if user.is_dir() {
        return Ok(user);
    }
    let stock = omarchy_share().join("themes").join(&slug);
    if stock.is_dir() {
        return Ok(stock);
    }
    Err(anyhow!("unknown theme: {slug}"))
}

/// Name of the theme currently applied, from Omarchy's own state file.
pub fn current_theme_name() -> Result<String> {
    let path = current_state()?.join("theme.name");
    let name = std::fs::read_to_string(&path)
        .map_err(|e| anyhow!("cannot read {}: {e}", path.display()))?;
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(anyhow!("{} is empty", path.display()));
    }
    Ok(name)
}

/// Render a path relative to `$HOME` as `~/...` for readable output.
pub fn tilde(path: &Path) -> String {
    match home() {
        Ok(h) => match path.strip_prefix(&h) {
            Ok(rest) => format!("~/{}", rest.display()),
            Err(_) => path.display().to_string(),
        },
        Err(_) => path.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::slugify;

    #[test]
    fn slugify_matches_omarchy_normalisation() {
        assert_eq!(slugify("Gojo Latte"), "gojo-latte");
        assert_eq!(slugify("gojo-latte"), "gojo-latte");
        assert_eq!(slugify("Your_Lie In April"), "your-lie-in-april");
        assert_eq!(slugify("<b>Tokyo Night</b>"), "tokyo-night");
    }
}
