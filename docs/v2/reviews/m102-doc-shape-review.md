# Adversarial doc-shape review — M102 (GitHub App connector / agent-identity-proxy, "Approach A") vs `docs/architecture/*`

**Author:** Orly (CTO mode) · **Date:** Jun 26, 2026 · **Feeds:** M102_001 §8 docs sweep (Files-Changed)
**Scope reviewed:** `user_flow.md`, `data_flow.md`, `high_level.md`, `capabilities.md`, `roadmap.md`, `README.md`, `runner_fleet.md`, `fleet_bundles.md`, `scenarios/github-pr-reviewer.md`

---

## Verdict

1. **Spec:** authored **new M102** (`docs/v2/active/M102_001_P1_API_CLI_UI_AGENT_IDENTITY_PROXY_GITHUB_APP.md`); did NOT revive M99 (its own deferral note forbids it). M99 = provenance.
2. **Doc shape today is COHERENT, and that is the problem.** The current docs consistently encode ONE model — *user manually registers a per-repo webhook from their own machine; platform holds no GitHub credential; the GitHub credential is a pasted token; the receiver verifies a workspace-pasted secret on a per-fleet URL.* No internal contradiction among them. Approach A overturns that model on the GitHub-App axis, so the change is **cross-cutting**: the docs must move together or they fall out of sync.
3. **The credential-boundary invariants HOLD.** Secrets-never-in-context, substitute-at-tool-bridge, no-secret-in-frames, no-new-trust-plane (reuse `agt_r`) all survive Approach A unchanged. The shape change is about *provenance + registration*, not the boundary.

---

## Shape ownership — who is the source of truth for each concept

| Concept | Canonical owner | Echoed by (must follow the owner) |
|---|---|---|
| Trigger / webhook ingress surface | **`user_flow.md` §8.3** | `data_flow.md` §B, `README.md` glossary, `high_level.md` §5.1, `capabilities.md` §3, `scenarios/github-pr-reviewer.md` |
| Lease→execute→report + secret resolution | **`data_flow.md` §C** | `capabilities.md` §3, `runner_fleet.md` |
| Credential model (vault + substitution) | **`capabilities.md` §2–3** | `data_flow.md` §C step 4, `user_flow.md` §8.5, `scenarios/github-pr-reviewer.md` §3 |
| Trust planes (daemon/runner/child, `agt_r`) | **`runner_fleet.md`** | `data_flow.md` |
| Wedge / flagship | **`high_level.md` §1, §5.1** | `user_flow.md` §8.0, §8.2.3 |
| Glossary | **`README.md`** | — |

**Update order:** fix the OWNERS first (`user_flow.md` §8.3, `data_flow.md` §B/§C, `capabilities.md` §2–3), then sweep the ECHOES (`README.md` glossary, `high_level.md` §5.1, `scenarios/github-pr-reviewer.md`). Editing an echo before its owner guarantees drift.

---

## Contradiction map

Change type: **OVERRIDE** (flips a documented fact) · **ADDITIVE** (new path beside the old) · **SILENT** (doc doesn't mention it → reads as "only the old way").

| # | Finding | Type | Severity | Hits (file:line) | Fix |
|---|---|---|---|---|---|
| **C1** | **Webhook registration ownership flips.** Today the *user* registers a per-repo hook (`gh auth login -s admin:repo_hook`, `gh api …/hooks`). GitHub App = ONE App-level webhook set once by the platform; install is the only user action; no per-customer registration. | OVERRIDE | **HIGH** | `user_flow.md`:94, :108, :153, :160, :175; `data_flow.md`:465; `README.md`:63; `scenarios/github-pr-reviewer.md`:32, :66; `high_level.md`:168 | Rewrite the registration steps to "install the App; GitHub auto-delivers." Drop `admin:repo_hook` + `gh api …/hooks` from the GitHub path. |
| **C2** | **Webhook URL shape:** per-fleet `/v1/webhooks/{fleet_id}/{source}` (fleet from path) → ONE App-level ingress, fleet derived `installation_id → workspace → fleet`. | OVERRIDE (per-fleet URL stays for generic/custom hooks) | **HIGH** | `README.md`:62; `user_flow.md`:133; `data_flow.md`:447; `scenarios/github-pr-reviewer.md`:34, :67; `high_level.md`:168 | Add the App ingress + installation_id routing; keep the per-fleet URL documented for non-App custom webhooks. |
| **C3** | **Webhook secret source:** workspace-pasted `fleet:github.webhook_secret` → ONE platform-level App webhook secret. Changes what the HMAC verifier reads + the `UZ-WH-020` user-recovery path ("fix with `credential add`"). | OVERRIDE | **HIGH** | `scenarios/github-pr-reviewer.md`:65, :67; `data_flow.md`:449, :506-510; `user_flow.md`:176 | App secret verifies App traffic. Reword UZ-WH-020 recovery (it no longer means "paste a secret" for the App path). |
| **C4** | **Credential model:** pasted `{token:"ghp_…"}` resolved statically → vault stores a HANDLE `{kind:"github_app", installation_id}` (no token); `resolveSecretsMap` emits a mintable handle; the bridge fetch-mints. Static custom-secrets remain a first-class kind. | ADDITIVE but docs are SILENT | **MED-HIGH** | `capabilities.md`:42, :57; `data_flow.md`:554; `user_flow.md`:181; `scenarios/github-pr-reviewer.md`:65, :73 | Document the `static` vs `mintable` kind split; vault stores a handle for mintable. |
| **C5** | **Platform now holds a GitHub-acting credential** (the App PRIVATE KEY, one, platform-side). Docs repeatedly assert "the platform never holds the user's PAT." Literally still true (it's not the *user's* PAT) but the spirit flips. | OVERRIDE (narrative) | **MED** | `README.md`:63; `user_flow.md`:108, :160, :175 | Reword: platform holds *its own App identity key* (KMS-sealed, admin vault), never a *user* token. Distinguish "App key (platform identity)" from "user PAT." |
| **C6** | **New intra-run mint round-trip** child→runner→daemon `POST /v1/runners/me/credentials/mint`. `data_flow.md` §C shows lease→run→report with NO mid-run call back to the daemon. Reuses `agt_r` (no new trust plane). | ADDITIVE / SILENT | **MED** | `data_flow.md`:575-608; `capabilities.md`:54; `runner_fleet.md` (agt_r routes) | Add the mint sub-flow to §C EXECUTE + one route to the `agt_r` plane in `runner_fleet.md`. |
| **C7** | **Wedge coherence.** `high_level.md` flagship = **platform-ops** (Fly/Upstash/Slack + GH-Actions-failure). M102's first driver = **GitHub App** — serves the github-pr-reviewer scenario AND the GH-Actions-failure trigger, but Fly/Upstash/Slack stay pasted (`oauth_refresh` deferred). Pre-existing mild tension: high_level=platform-ops vs user_flow=github-pr-reviewer-as-install-example. | SCOPING | **MED** | `high_level.md`:52, :168; `user_flow.md`:117, :126 | M102 states explicitly: GitHub App is the first/only minted connector; Fly/Upstash/Slack remain `static` until `oauth_refresh`. |
| **C8** | **`roadmap.md` silent on the connector/broker/agent-identity-proxy direction.** M99 deferred + "reborn as agent identity proxy" — no roadmap entry. | SILENT (gap) | **LOW-MED** | `roadmap.md` (whole) | Add a forward entry (or move M102 into active canon once it starts). |
| **C9** | **Glossary gaps.** No entries for connector / credential broker / agent identity proxy / integration grant / App installation. The "Trigger panel" entry actively asserts the old model. | SILENT + OVERRIDE | **LOW** | `README.md`:62-63 | Add glossary entries; reword Trigger panel. |

---

## Pre-existing tensions worth disambiguating in M102 (not caused by us, but we touch them)

- **"GitHub never on the runtime path"** (`scenarios/github-pr-reviewer.md`:7, :102) refers to the bundle SOURCE (tarball fetched once at install), but §4:73 has the runner calling `api.github.com` at runtime for the diff. Technically consistent (source ≠ API) but reads as a contradiction. M102 makes the runtime API call mint-backed, so the reborn scenario should split "source (one-time)" from "API (runtime, minted)" explicitly.
- **Onboarding lineage:** docs already migrated skill-auto-registration → user-manual-registration (`user_flow.md`:37 transitional note). Approach A is a THIRD model (App-native, zero registration) — neither of the two the docs describe. Name it as such so a reader isn't told the transitional note already covers it.

---

## Docs to update (M102 §8 docs sweep)

1. `user_flow.md` — §8.2.1 (drop `admin:repo_hook`), §8.2.2 step 6, §8.3, §8.4 (Trigger panel + step 4), §8.5 step 1–2. **[owner]**
2. `data_flow.md` — §B WEBHOOK (ingress + secret), §C step 4 (resolve-or-mint) + EXECUTE (mint sub-flow), UZ-WH-020 taxonomy. **[owner]**
3. `capabilities.md` — §2 http_request (mintable), §3 vault (handle vs token) + per-lease policy (mint). **[owner]**
4. `scenarios/github-pr-reviewer.md` — §3 (Connect, not register), §4 (mint-backed call), §8 (source vs API split). **[echo]**
5. `high_level.md` — §5.1 webhook bullet; state the connector/flagship relationship. **[echo]**
6. `README.md` — glossary (Trigger panel reword + new terms). **[echo]**
7. `runner_fleet.md` — one mint route on the `agt_r` plane. **[echo]**
8. `roadmap.md` — connector/agent-identity-proxy entry. **[gap]**

**Invariants that DO NOT change (asserted in the M102 spec so a reviewer can check):** secrets-never-in-fleet-context; substitute-at-tool-bridge; no-secret-in-frames; no new trust plane (`agt_r` reused); static custom-secrets remain first-class; one-active-lease + fencing; approval gate stays control-plane + poll/continuation.
