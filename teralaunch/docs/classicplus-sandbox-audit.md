# ClassicPlus Launcher Sandbox Audit

Date: 2026-04-28

## Scope

This audit checks whether the ClassicPlus launcher keeps its application identity, local storage, config files, updater, protocol handler, and mod metadata separate from the classic launcher. The local classic launcher path requested for direct comparison, `C:\Users\Lukas\Documents\GitHub\TERA EU Classic\TERA-Germany-Launcher`, was not present on this machine, so this audit is based on the ClassicPlus repository evidence only.

## Findings

| Area | ClassicPlus value | Status |
| --- | --- | --- |
| Tauri identifier | `crazy-esports-classicplus` in `src-tauri/tauri.conf.json` | Isolated from a non-ClassicPlus Tauri app identifier. |
| Product/binary name | `TERA Europe Classic+ Launcher` / `TERA Europe Classic+ Launcher` | Isolated installer and executable display name. |
| Windows install mode | NSIS `perMachine` | Installer is machine-wide; identity still separates app metadata. |
| Updater endpoint | `https://web.tera-germany.de/classic/classicplus/latest.json` | Uses ClassicPlus-specific update manifest path. |
| Deep link protocol | `teraclassicplus://` under `HKCU\Software\Classes\teraclassicplus` | Isolated protocol/registry key. |
| Config file | `%APPDATA%\Crazy-eSports-ClassicPlus\tera_config.ini` via `dirs_next::config_dir()` | Isolated from classic config directories. |
| Mods metadata/cache | `%APPDATA%\Crazy-eSports-ClassicPlus\mods\...` | Isolated from classic mod metadata/cache directories. |
| Frontend storage | Tauri WebView local/session storage scoped by app identity; keys include `tera_accounts`, `_cred`, `authKey`, `classicplus_consent_prompt_version` | App identity is ClassicPlus-specific; consent prompt key is also ClassicPlus-specific. |
| CSP/network allowlist | Includes `https://tera-europe-classic.com` and ClassicPlus/tester-relevant endpoints | Consent calls are allowed without broadening to arbitrary origins. |

## Consent-specific sandboxing

The ClassicPlus launcher now treats ClassicPlus as tester for leaderboard consent:

- Frontend consent reads/writes call Tauri commands `get_leaderboard_consent` and `set_leaderboard_consent`.
- Rust commands authenticate against tester website routes:
  - `/api/tester/auth/session`
  - `/api/tester/auth/login/start`
  - `/api/tester/auth/settings/consent`
- The HTTP client uses HTTPS-only defaults and a cookie store for the CSRF/session flow.
- A version-gated prompt key, `classicplus_consent_prompt_version`, forces the first-launch consent modal for this ClassicPlus consent release and is only marked seen after a successful consent write.

## Notes / risks

- The launcher still stores password credentials in WebView `localStorage` as base64 (`_cred` and `tera_accounts.credentials`). This appears pre-existing and is not a sandbox collision, but it is not strong credential protection.
- Direct proof against the classic launcher remains pending until the classic launcher repository path is available locally.
- Rust LSP diagnostics could not run in this environment because `rust-analyzer.exe` is missing from the stable toolchain; `cargo test --test consent_commands_guard` compiled the touched Rust path successfully.
