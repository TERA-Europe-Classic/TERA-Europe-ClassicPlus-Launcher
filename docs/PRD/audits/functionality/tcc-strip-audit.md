# TCC strip audit

Scope: classify the user-facing and infrastructure features removed by strip
commit `88e6fe30` as **RESTORED**, **OUT-OF-SCOPE**, or **DEFERRED** per PRD
`3.3.8`.

Audit date: 2026-04-22

Reference commit:

- `88e6fe30 feat: Classic+ read-only fork — strip every write path, add mirror sniffer`

---

## Decision table

| Removed/changed feature | Current state | Verdict | Justification |
|---|---|---|---|
| Classic+ mirror sniffer replacing toolbox sniffer | `TeraPacketParser/Sniffing/ClassicPlusSniffer.cs` present | RESTORED | Required Classic+ functionality and already live in the fork. |
| External app launch/build path without `TCC.Loader` / `TCC.Modules` | `TCC.Core` standalone build path present | RESTORED | Current fork builds and ships as a standalone WinExe. |
| LFG write path (listing creation / application / LFG windows) | removed and replaced with stubs in `ClassicPlusStubs.cs` | OUT-OF-SCOPE | PRD explicitly keeps LFG-write stubbed in the current milestone. |
| Moongourd parse lookups / popup | removed and stubbed | OUT-OF-SCOPE | PRD explicitly keeps Moongourd dependencies stubbed. |
| Firebase / Cloud telemetry | removed and stubbed in `ClassicPlusStubs.cs` | OUT-OF-SCOPE | PRD explicitly keeps Firebase / Cloud telemetry stubbed. |
| Proxy RPC / toolbox JSON-RPC write surfaces | removed / stubbed | OUT-OF-SCOPE | Read-only Classic+ model intentionally excludes write-path RPC plumbing. |
| Friend-message dialog write flow | deleted; only compile stub remains | OUT-OF-SCOPE | This is a write-path feature and not required by the current Classic+ launcher scope. |
| Discord webhook delivery | `TCC.Interop/Discord.cs` still no-op; `SettingsWindowViewModel` still exposes webhook registration command | DEFERRED | PRD `3.3.7` explicitly expects Discord webhook integration restored, but runtime webhook delivery is still stubbed. |
| LFG-related settings types / windows preserved only as compile shims | stubbed in `ClassicPlusStubs.cs` | OUT-OF-SCOPE | Necessary compile/persistence shims only; feature itself remains intentionally absent. |

---

## Key evidence

### Restored / present

- `ClassicPlusSniffer.cs` exists and is the active sniffer path in the fork.
- `TCC.Core` currently builds as a standalone application without the upstream
  loader/module sibling repos.

### Explicitly stubbed by PRD

- `docs/PRD/mod-manager-perfection.md` non-goals keep **Moongourd / Firebase /
  LFG-write / Cloud telemetry** stubbed.
- `ClassicPlusStubs.cs` contains compile-time no-op shims for those stripped
  surfaces.

### Still deferred / missing

- `TCC.Interop/Discord.cs` currently documents `FireWebhook` as an intentional
  no-op.
- `SettingsWindowViewModel` still exposes `RegisterWebhookCommand`, meaning the
  settings/UI seam exists, but actual outbound webhook behavior has not been
  restored yet.

---

## Conclusion

The strip pass is **mostly aligned** with the current Classic+ product scope,
but not completely finished relative to the PRD:

- write-path systems like LFG, Moongourd, Firebase, and Cloud are correctly
  classified as out-of-scope for the current milestone,
- core read-only fork functionality (mirror sniffer / standalone runtime) is
  restored,
- **Discord webhooks remain the major deferred gap** because the PRD expects
  them restored while the current code still stubs delivery.

Status: **PARTIAL — one user-facing feature family still deferred**
