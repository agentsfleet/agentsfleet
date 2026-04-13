# Ripley's Log — /review PR #204 — Apr 12, 2026: 5:45 PM

**Branch:** feat/m8-slack-plugin  
**PR:** usezombie/usezombie#204  
**Session scope:** Continued from context-limit handoff; all P1/P2 bug fixes committed, /review completed.

---

## What happened this session

Picked up mid-review after context compaction. Four auto-fix commits were already pushed (7f912bb, 0e27a29, b87103f) from the previous session. Resume point was the /review skill execution.

**Fixes added this session:**

1. `slack_events.zig` — stale comment update (commit bd2b66a)
   - Line 9: "HTTP 200 always" was incorrect after the 503-on-infra-failure fix
   - Added `subtype=bot_message` test — the §4.0 bot loop check was implemented but had no test for the `bot_id == null, subtype = bot_message` code path
   - 839/904 tests pass after (was 837/902)

2. `webhook_verify.zig` + `slack_oauth.zig` — P2 fixes (commit d07b6de)
   - `isTimestampFresh` rejected past timestamps but accepted future timestamps within the drift window. Fixed to reject `ts > now + max_drift` — prevents pre-signed request attacks.
   - `extractWorkspaceId` used `catch ""` on OOM, silently falling through to create a new workspace instead of linking to the existing one. Changed return type to `![]const u8`; caller now handles with a 500.

---

## Specialist review findings — triage

Three agents ran in parallel (critical pass, security, test coverage). Key triage decisions:

**False positives rejected:**
- Security agent "CRITICAL — panic on zero-length in HMAC for-loop": False positive. Line 193 has an explicit `if (provided.len != 64) return false` before the for-loop. No panic possible.
- Critical agent "P2 — catch unreachable on nonce bufPrint": Provably safe. Buffer is 64 bytes; key is `"slack:oauth:nonce:"` (18 chars) + 32-char hex = 50 chars. Can't overflow.
- Critical agent "P2 — interactions TOCTOU on exists+resolveApproval+DEL": False positive. `resolveApproval` is a single `SETEX` — idempotent. Concurrent duplicate calls write the same value; no harmful consequence.

**Real findings fixed:**
- Future-timestamp bypass (P2) — webhook_verify.zig → fixed
- Silent OOM masking in extractWorkspaceId (P2) — slack_oauth.zig → fixed

**Accepted as known/deferred:**
- `upsertIntegration` always returns `.created = false` (P3) — no current caller uses the flag; safe to defer
- Auth ownership on /install (P1) — deferred; frontend coordination required (documented in prior session)

---

## Greptile threads replied (5 total)

| Comment ID | Path | Verdict | Reply ID |
|------------|------|---------|----------|
| 3069632352 | slack_oauth.zig | Acknowledged / deferred | 3069795335 |
| 3069632383 | slack_events.zig | Fixed in 7f912bb | 3069794690 |
| 3069632399 | workspace_integrations.zig | Fixed in 0e27a29 | 3069794843 |
| 3069632415 | slack_oauth.zig | False positive (bootstrap §2.0) | 3069795103 |
| 3069792046 | slack_interactions.zig | False positive (resolveApproval idempotent) | 3069795596 |

---

## Rules added to RULES.md

- **RULE TWF** — Timestamp freshness must reject future timestamps
- **RULE ESO** — Error returns must not silently substitute default values on OOM

---

## Decisions and trade-offs

**Why not GETDEL for interactions?** The `exists` check before `resolveApproval` is a perf optimization — avoids a Redis write on expired/already-processed gates. Since `resolveApproval` is idempotent (SETEX), a concurrent duplicate write has no harmful effect. GETDEL would require changing the pending key format to store the action payload, adding complexity without a real safety benefit. P3 at best.

**Why `max_drift` tolerance for future timestamps?** Slack's clock and our server clock can drift. A strict `ts <= now` would cause false rejections from Slack if our server is 1-2 seconds behind. The `max_drift` (300s, Slack default) as the forward tolerance is excessive but matches the backward tolerance — real-world value is whatever clock synchronization guarantees. Could tighten to 30s in a follow-up.

---

## State at session end

- PR #204 is open, all commits pushed, all Greptile threads replied
- Auth ownership (#1) remains deferred — needs frontend PR
- 839/904 tests pass, 65 skipped, 0 failures from M8 code
- Cross-compile: x86_64-linux ✓, aarch64-linux ✓
- pg-drain: ✓ (207 files)
