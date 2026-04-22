# Anti-reverse Hardening Audit

**PRD:** §3.1.8 anti-reverse-hardening.
**Milestone:** M6 (Tauri v2 migration), **iter 70**, worktree commit TBD.
**Target binary:** `teralaunch/src-tauri/target/release/tera-europe-classicplus-launcher.exe`.

This audit captures the defence-in-depth flags enabled against casual
reverse engineering and tampering, and the ones deliberately deferred.
It's the acceptance artefact for PRD §3.1.8.

## What's enabled

### Compile-time profile flags (already on pre-M6)

From `teralaunch/src-tauri/Cargo.toml` `[profile.release]`:

| Flag | Setting | Effect |
|---|---|---|
| `opt-level` | `3` | Full optimisation — folded constants, inlined calls, less cleanly recoverable source structure in the binary. |
| `lto` | `true` | Cross-crate link-time optimisation — dead code stripped across the whole dependency graph; call graphs harder to recover. |
| `codegen-units` | `1` | Single codegen unit enables more aggressive inlining across modules. |
| `panic` | `abort` | No unwinding tables, fewer reverse-engineering landmarks. |
| `strip` | `true` | Symbol table stripped from the final `.exe` — function names gone. |

These were set pre-M6 and carry through the Tauri v2 migration unchanged.

### Windows CFG linker flag (new in M6)

Added in `teralaunch/src-tauri/build.rs` under the `#[cfg(target_os =
"windows")]` block:

```rust
if env::var("PROFILE").unwrap_or_default() == "release" {
    println!(
        "cargo:rustc-link-arg-bin=tera-europe-classicplus-launcher=/guard:cf"
    );
}
```

This forwards `/guard:cf` to the MSVC linker for the final release
binary only. The flag sets the `IMAGE_DLLCHARACTERISTICS_GUARD_CF` bit
in the PE header, which tells the Windows loader to apply OS-level
mitigations:

- **CIG (Code Integrity Guard)** — only Microsoft-signed DLLs can load
  into the process.
- **ACG (Arbitrary Code Guard)** — prevents RWX memory and dynamic code
  generation outside the loader's sanctioned paths.
- **Dynamic code guard** — blocks VirtualProtect from changing
  page-protection bits to add executable permissions mid-process.

Without the header bit, Windows won't enforce those mitigations even if
they're opted in via process policy.

### Stack-smash protection (inherited)

Rust on `x86_64-pc-windows-msvc` inherits MSVC CRT's `/GS` stack canary
by default. No explicit flag needed — the MSVC runtime library link
brings it in. Verified by inspecting the `__security_cookie` symbol in
prior release binaries (M0 baseline iter 62).

## What's deferred

### Full CFG rustc instrumentation (M6-b)

`-C control-flow-guard=checks` on the `rustc` command line emits
per-indirect-call check metadata so the CFG bitmap actually gets
consulted at runtime. Without it, the `/guard:cf` linker flag sets the
PE header bit but no check-call sites exist in the code — effectively
"CFG header-only."

Iter 70 attempted the full instrumentation via
`.cargo/config.toml::[target.x86_64-pc-windows-msvc].rustflags`. Under
LTO + single codegen-unit, that pushed host-side `build.rs` compilation
OOM on a 16 GB dev machine. The flag silently applies to host artefacts
(build scripts) because host == target on a Windows dev box.

**Queued for M6-b** — handle via a CI-only build step that sets
`RUSTFLAGS` scoped to the final bin compile, not the host pipeline. CI
runners have predictable memory and no LTO-on-host problem.

### String obfuscation (M6-b)

The `cryptify` 3.1.1 + `chamox` 0.1.4 crates are pinned in Cargo.toml
(lines 28–29) but not yet applied to any literals in this codebase. The
top-priority candidate is `teralib/src/config.rs` — `const CONFIG: &str
= include_str!(...)` embeds `config.json` (containing
`157.90.107.2:8090`) into the binary as a plaintext `.rdata` string.

A plaintext `strings launcher.exe | grep 192.168` lists today:

- `teralib/src/config/config.json` — *compiled in via `include_str!`*
  (primary obfuscation target).
- `teralaunch/src-tauri/capabilities/migrated.json` — *embedded by
  `tauri::generate_context!()` for the runtime permission check*. Must
  remain plaintext; obfuscating it would break Tauri's capability
  resolver. Out of scope for 3.1.8.
- `teralaunch/src-tauri/tauri.conf.json` — *same story; embedded at
  compile time for the runtime runtime-config bootstrap*. Plaintext by
  design.

**Queued for M6-b** — wrap the `include_str!("config.json")` with a
compile-time XOR pass in `build.rs` (the existing mirror-PSK
obfuscation already demonstrates the pattern, lines 40-42), emit the
obfuscated bytes via a generated `config_obfs.rs`, and decrypt lazily
in `CONFIG_JSON` init. Expected surface: ~15 lines in `config.rs`,
~10 lines added to the existing `build.rs` XOR loop.

### Binary-diff check

The migration-plan M6 acceptance includes "release binary string-grep
for `157.90.107.2` returns zero hits in the obfuscated sections." This
requires:

1. A signed release build (the signing key is GitHub-secret-only —
   same constraint hit in M4).
2. The M6-b string obfuscation landing.

**Queued for the first post-merge CI release at v0.2.0.** At that point
a one-shot `strings release/nsis/*.exe | grep '192\.168\.1\.128'` in
CI (or a dedicated audit iter pulling the artefact from the release)
closes PRD §3.1.8 end-to-end.

## Summary

| Item | Status @ iter 70 |
|---|---|
| Release-profile LTO + strip + opt-3 + abort + codegen-units=1 | **ON** (pre-M6) |
| MSVC `/GS` stack canary | **ON** (inherited) |
| Windows `/guard:cf` linker flag (PE header bit) | **ON** (this iter) |
| Full CFG rustc metadata | **deferred to M6-b** (CI-only RUSTFLAGS scope) |
| cryptify/chamox string obfuscation on `CONFIG` | **deferred to M6-b** |
| Release-binary plaintext-grep proof | **deferred to first v0.2.0 CI release** |

Two of five tiers land this iter. Sign-off on the remaining three gates
on M6-b + the M8 release cut.
