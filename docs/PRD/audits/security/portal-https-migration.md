# Portal API HTTPS Migration

**Criterion:** PRD §3.1.13.
**Status:** Draft — pending production HTTPS endpoint deployment.
**Last updated:** iter 9.

## Current state (dev-only)

`teralib/src/config/config.json` points at a LAN IP over plain HTTP:

| Key | URL |
|---|---|
| `API_BASE_URL` | `http://157.90.107.2:8090` |
| `LOGIN_ACTION_URL` | `http://157.90.107.2:8090/tera/LauncherLoginAction` |
| `GET_ACCOUNT_INFO_URL` | `http://157.90.107.2:8090/tera/GetAccountInfoByUserNo` |
| `REGISTER_ACTION_URL` | `http://157.90.107.2:8090/tera/LauncherSignupAction` |
| `MAINTENANCE_STATUS_URL` | `http://157.90.107.2:8090/tera/LauncherMaintenanceStatus` |
| `SERVER_LIST_URL` | `http://157.90.107.2:8090/tera/ServerList` |
| `PATCH_SOURCE` | `v100_static` |
| `V100_PATCH_BASE_URL` | `http://157.90.107.2:8090/public/patch` |

This is the developer's LAN. It is not externally routable. Every request carries the launcher's `AuthKey` and password plaintext over the wire.

## Threat model

Over HTTP on an unsecured network:

- **Credential capture** — password at `/tera/LauncherLoginAction`, `AuthKey` on subsequent `/tera/GetAccountInfoByUserNo` POSTs.
- **Session hijack** — MitM replays `AuthKey` against the same endpoint for the lifetime of the session.
- **Response tampering** — attacker can rewrite `MAINTENANCE_STATUS`, `SERVER_LIST`, or maintenance banner text, steering the client anywhere.
- **Patch tampering / resource exhaustion** — attacker can rewrite `version.ini`, the v100 SQLite DB CAB, or patch CAB payloads under `V100_PATCH_BASE_URL`; launcher-side URL scoping, path traversal rejection, size ceilings, and final hash checks are defense-in-depth, not a substitute for HTTPS/authenticated patch metadata.
- **Stale-TLS-pinning absence** — even if the client later moves to HTTPS, without cert-pinning the launcher will trust any CA-signed cert for the target host.

## Required before public launch

Deploy the portal behind TLS. Minimum viable:

1. **FQDN** — e.g. `portal.classicplus.tera-europe-classic.org` (or equivalent). DNS A/AAAA record pointing at the production host.
2. **TLS certificate** — Let's Encrypt or commercial. 90-day ACME renewal automation.
3. **Reverse proxy** — Caddy / nginx / Cloudflare tunnel terminating TLS at the host, proxying to the existing `:8090` backend on localhost.
4. **HSTS** — `Strict-Transport-Security: max-age=31536000; includeSubDomains; preload` on every response.
5. **TLS 1.2+ only**, disable SSLv3/TLS 1.0/1.1, modern cipher suite.

## Launcher-side migration steps (once the endpoint is up)

1. Update `teralib/src/config/config.json` — replace every `http://157.90.107.2:8090` with `https://<fqdn>`, including `V100_PATCH_BASE_URL`.
2. Run full auth flow (login → account info → server list → maintenance status → registration) against the new endpoint. Each must return the documented `Return: true, ReturnCode: 0` shape.
3. Verify `reqwest` in `teralaunch/src-tauri` actually terminates TLS (no custom `danger_accept_invalid_certs(true)` in the codepath — grep confirmed none today).
4. Optional but recommended: add a CI gate that fails `cargo build --release` if any URL in `config.json` starts with `http://` (rg-based check in `.github/workflows/deploy.yml`). Tracked separately as a candidate polish.
5. Consider cert-pinning via `reqwest::ClientBuilder::add_root_certificate` once the chosen issuer is settled.

## Rollback plan

Config-only change: revert the commit that flipped the URLs and redeploy. No schema, no database, no state migration. Rollback window: however long between the flip and the first passing end-to-end check.

## Acceptance (per PRD §3.1.13)

- [ ] Config URL starts with `https://`.
- [ ] End-to-end login works against the HTTPS endpoint.
- [ ] Audit doc (this file) signed off.

All three remain open until the production HTTPS endpoint is deployed. **This item cannot close without that external milestone.**

## Human input required

The human decides and provides:

1. The production FQDN.
2. Whether TLS termination is direct on the host, Cloudflare tunnel, or another CDN.
3. Whether cert-pinning is required for v1 or deferred.

Until those three are known, the config flip cannot be authored and the audit cannot be signed off. The loop will re-attempt this item on every BLOCKED RE-TRY iteration (every 50).
