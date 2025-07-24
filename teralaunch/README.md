# Tauri + Vanilla

This template should help get you started developing with Tauri in vanilla HTML, CSS and Javascript.

## Recommended IDE Setup

- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)

## Configuration File Location

The launcher now stores `tera_config.ini` in your operating system's configuration
directory. On first run the file is created automatically using the bundled
defaults.

If a legacy configuration exists in `%APPDATA%/Crazy-eSports.com` it will be
copied to the new location so existing settings are preserved.

- **Windows:** `%APPDATA%/crazy-esports/tera_config.ini`
- **Linux:** `$XDG_CONFIG_HOME/crazy-esports/tera_config.ini` or
  `~/.config/crazy-esports/tera_config.ini`
- **macOS:** `~/Library/Application Support/crazy-esports/tera_config.ini`

The configuration is stored per-user, so the launcher can run without
administrative privileges regardless of the install directory.

## Windows Code Signing

Some antivirus programs may flag unsigned executables. To avoid this the
Windows build should be code signed. Set the environment variable
`WINDOWS_CERT_THUMBPRINT` to the thumbprint of your code signing certificate
before running `npm run tauri build`. The build script will pass this value to
Tauri which signs the installer automatically using `signtool`.
