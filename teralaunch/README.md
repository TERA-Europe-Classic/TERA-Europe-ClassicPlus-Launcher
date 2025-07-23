# Tauri + Vanilla

This template should help get you started developing with Tauri in vanilla HTML, CSS and Javascript.

## Recommended IDE Setup

- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)

## Configuration File Location

The launcher now stores `tera_config.ini` in your operating system's configuration
directory. On first run the file is created automatically using the bundled
defaults.

If a `tera_config.ini` is found in the old launcher directory it will be moved
to the new location so existing settings are preserved.

- **Windows:** `%APPDATA%/Crazy-eSports.com/tera_config.ini`
- **Linux:** `$XDG_CONFIG_HOME/Crazy-eSports.com/tera_config.ini` or
  `~/.config/Crazy-eSports.com/tera_config.ini`
- **macOS:** `~/Library/Application Support/Crazy-eSports.com/tera_config.ini`
