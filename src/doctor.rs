use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::paths;

/// Report colour literals in the user's config that duplicate a colour of the
/// current theme. Those are exactly the values that will not follow the next
/// theme switch — the class of bug that pinned hyprbars to one palette.
pub fn run() -> Result<bool> {
    let theme = paths::current_theme_name()?;
    let colors_path = paths::current_state()?.join("theme/colors.toml");
    let palette = load_palette(&colors_path)?;

    println!("omatheme doctor — theme {theme}");
    println!(
        "  palette: {} keys from {}",
        palette.len(),
        paths::tilde(&colors_path)
    );

    let mut findings = 0usize;
    for file in scan_targets()? {
        let contents = match std::fs::read_to_string(&file) {
            Ok(contents) => contents,
            Err(_) => continue,
        };
        for (number, line) in contents.lines().enumerate() {
            if line.trim_start().starts_with("--") || line.trim_start().starts_with("//") {
                continue;
            }
            for hex in hex_literals(line) {
                if let Some(key) = palette.get(hex.as_str()) {
                    println!(
                        "  {}:{}: {hex} duplicates `{key}` of the theme — should come from a template",
                        paths::tilde(&file),
                        number + 1
                    );
                    findings += 1;
                }
            }
        }
    }

    if findings == 0 {
        println!("  no hardcoded theme colour found");
    } else {
        println!("  {findings} literal(s) to move into ~/.config/omarchy/themed/*.tpl");
    }
    Ok(findings == 0)
}

/// Files a user is likely to hand-edit with colours in them.
fn scan_targets() -> Result<Vec<PathBuf>> {
    let mut targets = Vec::new();
    let config = paths::config_home()?;
    for dir in [config.join("hypr"), config.join("omarchy")] {
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if path.is_file() && matches!(ext, "lua" | "conf" | "json" | "jsonc" | "toml") {
                targets.push(path);
            }
        }
    }
    targets.sort();
    Ok(targets)
}

/// `#rrggbb`, `rgb(rrggbb)` and `rgba(rrggbbaa)` all normalise to `rrggbb`.
fn hex_literals(line: &str) -> Vec<String> {
    let bytes: Vec<char> = line.chars().collect();
    let mut found = Vec::new();
    let mut index = 0usize;

    while index < bytes.len() {
        let run_start = index;
        while index < bytes.len() && bytes[index].is_ascii_hexdigit() {
            index += 1;
        }
        let len = index - run_start;
        if len == 6 || len == 8 {
            let boundary_before = run_start
                .checked_sub(1)
                .map(|i| matches!(bytes[i], '#' | '(' | '"' | '\'' | ' ' | '='))
                .unwrap_or(true);
            let boundary_after = bytes
                .get(index)
                .map(|c| !c.is_alphanumeric() && *c != '_')
                .unwrap_or(true);
            if boundary_before && boundary_after {
                let hex: String = bytes[run_start..run_start + 6].iter().collect();
                found.push(hex.to_ascii_lowercase());
            }
        }
        if index == run_start {
            index += 1;
        }
    }
    found
}

/// `colors.toml` is flat `key = "#rrggbb"`; index it the other way round so a
/// literal can be traced back to the key it duplicates.
fn load_palette(path: &Path) -> Result<BTreeMap<String, String>> {
    let raw =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let mut palette = BTreeMap::new();

    for line in raw.lines() {
        let line = line.trim();
        if line.starts_with('#') || !line.contains('=') {
            continue;
        }
        let (key, value) = match line.split_once('=') {
            Some(pair) => pair,
            None => continue,
        };
        let key = key.trim();
        // The value may carry a trailing comment (`accent = "#3a6fb5" # gojo`),
        // so read the six hex digits that follow the first `#` instead of
        // splitting the line.
        let value = value.trim();
        let Some(hash) = value.find('#') else {
            continue;
        };
        let hex: String = value[hash + 1..].chars().take(6).collect();
        if hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
            palette
                .entry(hex.to_ascii_lowercase())
                .or_insert_with(|| key.to_string());
        }
    }
    Ok(palette)
}

#[cfg(test)]
mod tests {
    use super::hex_literals;

    #[test]
    fn reads_every_colour_notation() {
        assert_eq!(hex_literals(r#"bar_color = "rgb(dfcfaa)""#), vec!["dfcfaa"]);
        assert_eq!(hex_literals(r#"col = "rgba(595959aa)""#), vec!["595959"]);
        assert_eq!(hex_literals(r##"accent = "#3A6FB5""##), vec!["3a6fb5"]);
    }

    #[test]
    fn ignores_identifiers_and_short_runs() {
        assert!(hex_literals("local deadbeef_value = 1").is_empty());
        assert!(hex_literals("size = 12").is_empty());
    }
}
