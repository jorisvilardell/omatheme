# omatheme

Theme *profiles* for [Omarchy](https://omarchy.org/).

Omarchy 4 already renders your theme's palette into every app — terminals,
btop, Neovim, VS Code, Chromium, the Quickshell bar, Hyprland's window
borders — through the template engine in `~/.config/omarchy/themed/*.tpl`.
What it does not carry is everything about a theme that is *not* a colour:
which wallpaper is the right one, which font goes with it, whether the bar
should be transparent, which menu to use.

omatheme adds exactly that, and nothing else. It never touches colours; it
reads a `profile.toml` you drop inside a theme directory and applies it every
time you switch themes.

## Why not one of the existing tools?

The Omarchy ecosystem has plenty of *palette generators* — they turn a
wallpaper into a `colors.toml`. None of them manage a theme as a bundle.

| Project | What it does | Gap |
| --- | --- | --- |
| [tema](https://github.com/bjarneo/tema) | Wallpaper → theme via ImageMagick | Unmaintained, Waybar-era, no Quickshell |
| [omarchist](https://github.com/tahayvr/omarchist) | GUI theme designer | Early stage, colours only |
| [omarchy-auto-theme](https://github.com/AccursedGalaxy/omarchy-auto-theme) | matugen Material You on wallpaper change | Live generation only, no persistent themes |
| [omarchy-theme-generate](https://github.com/ryrobes/omarchy-theme-generate) | Image → full theme | Pre-4.0 |

omatheme is complementary: generate your palette with any of the above, then
let omatheme carry the rest.

## Install

```bash
git clone https://github.com/jorisdev/omatheme
cd omatheme
cargo build --release
install -Dm755 target/release/omatheme ~/.local/bin/omatheme
omatheme install-hook
```

`install-hook` writes `~/.config/omarchy/hooks/theme-set.d/omatheme.hook`, so
every `omarchy theme set` — from the CLI, the menu or a keybinding — applies
the new theme's profile.

## `profile.toml`

Drop it in the theme directory, next to `colors.toml`. Omarchy ignores the
file, so a theme with a profile stays a perfectly valid stock theme.

```toml
[wallpaper]
# Pin the wallpaper instead of letting Omarchy cycle alphabetically.
default = "backgrounds/1-glitch.jpg"

[font]
name = "CaskaydiaMono Nerd Font"

[shell]
# Dotted paths into ~/.config/omarchy/shell.json.
patch = { "bar.transparent" = true, "idle.lock" = 600 }

[menu]
# Replacement for ~/.config/omarchy/extensions/omarchy-menu.jsonc.
extension = "menu.jsonc"

[lock]
# Directory of presentational QML overlaid on a fresh clone of omarchy.lock.
overlay = "lock"

[launcher]
# What `omatheme launcher <verb>` forwards to. Bind SUPER + SPACE to
# `omatheme launcher toggle` once and the launcher follows the theme.
command = "omarchy-spiderverse-launcher"
bin = "launcher/bin/omarchy-spiderverse-launcher"
quickshell = { source = "launcher/quickshell", name = "spiderverse-launcher" }
autostart = true

[commands]
# Escape hatch, run last, with $OMATHEME_THEME and $OMATHEME_THEME_DIR set.
post = ["omarchy restart shell"]
```

Every field is optional, and every step is idempotent.

**Baselines.** `shell.json` and the menu extension are patched from an
untouched baseline (`shell.base.json`, `omarchy-menu.base.jsonc`, seeded on
first run), never from the previous theme's result. Switching from a theme
that sets `bar.transparent = true` to one that says nothing restores your
original value instead of inheriting it.

## Lock screen and launcher

On Omarchy Quattro the lock screen is a Quickshell *service* plugin, not
hyprlock. `Service.qml` holds the PAM and `ext-session-lock-v1` logic;
`LockView.qml` and what it pulls in are the visual layer. The only supported
way to restyle it is `omarchy plugin clone omarchy.lock`, which forks both
files — and a theme shipping its own `Service.qml` would freeze that security
logic at whatever Omarchy version it was authored against.

So omatheme reclones from the *installed* Omarchy on every switch and overlays
only the theme's presentational files on top. It marks the clone as its own
(`.omatheme-managed`), and on a theme with no `[lock]` it disables the clone,
deletes it, and re-enables `omarchy.lock` — the stock lock screen comes back
untouched. A clone you made by hand carries no marker and is never deleted.

The launcher works by indirection instead. `omatheme launcher <verb>` forwards
to the current theme's `command`, or to `omarchy-menu` when the theme declares
none, so one keybinding covers every theme:

```lua
hl.unbind("SUPER + SPACE")
o.bind("SUPER + SPACE", "Theme launcher", "omatheme launcher toggle")
```

Switching away stops the previous theme's launcher process.

Nothing here restarts the shell: it watches `~/.config/omarchy/plugins/` with
inotify and `shell.json` with a `FileView`, so plugin and config changes land
live. The one shell key omatheme never restores from the baseline is plugin
enablement (`plugins`, `disabledPlugins`) — the shell owns those.

## Commands

| Command | Effect |
| --- | --- |
| `omatheme apply <theme>` | Switch theme, then apply its profile |
| `omatheme sync` | Apply the current theme's profile without switching |
| `omatheme hook [theme]` | Entry point for the Omarchy `theme-set` hook |
| `omatheme install-hook` | Install this binary as that hook |
| `omatheme launcher <verb>` | Forward to the current theme's launcher |
| `omatheme new <name> --from <image>` | Scaffold a theme (colors, wallpaper, preview, profile) |
| `omatheme doctor` | Report colour literals that will not follow a theme switch |

`--dry-run` works on all of them.

### doctor

`doctor` is the reason this project exists. Hand-edited Hyprland config tends
to accumulate hex literals copied out of the current palette — they look right
until you switch themes:

```
$ omatheme doctor
omatheme doctor — theme gojo-latte
  palette: 24 keys from ~/.local/state/omarchy/current/theme/colors.toml
  ~/.config/hypr/looknfeel.lua:102: dfcfaa duplicates `darker_background` of the theme
  ~/.config/hypr/looknfeel.lua:103: 443f40 duplicates `foreground` of the theme
```

The fix is a user template. For the hyprbars title bar, that is
`~/.config/omarchy/themed/hyprbars.lua.tpl`:

```lua
return {
  bar_color = "rgb({{ darker_background_strip }})",
  text_color = "rgb({{ foreground_strip }})",
}
```

Omarchy renders it into `~/.local/state/omarchy/current/theme/hyprbars.lua` on
every theme switch, and `looknfeel.lua` consumes it:

```lua
local ok, theme_bars = pcall(require, "omarchy.current.theme.hyprbars")
if not ok or type(theme_bars) ~= "table" then theme_bars = {} end
local bar_color = theme_bars.bar_color or "rgb(2e2e2e)"
```

## Requirements

Omarchy 4.0+, Rust 1.85+. ImageMagick is optional (previews in
`omatheme new`).

## License

MIT
