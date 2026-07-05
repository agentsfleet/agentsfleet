<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the kishore-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M115_001: Connector connect-flow user guide, Zoho/Jira/Linear operator playbooks, and connector-doc drift reconcile

**Prototype:** v2.0.0
**Milestone:** M115
**Workstream:** 001
**Date:** Jul 05, 2026
**Status:** PENDING
**Priority:** P1 — customer- and operator-facing documentation; today the connect flow is undocumented and three of five providers have no operator runbook.
**Categories:** DOCS
**Batch:** B1 — standalone documentation workstream, no code dependency.
**Branch:** {feat/mNN-name — added at CHORE(open)}
**Test Baseline:** set at CHORE(open) — `unit=<N> integration=<M>` via `make _lint_zig_test_depth`
**Depends on:** none — connector platform shipped (M106, M108); this documents what exists.
**Provenance:** agent-generated (pre-spec, this session's connector documentation-gap audit)
**Canonical architecture:** `docs/architecture/connectors.md` §archetypes, §trust-anchors

---

## Overview

**Goal (testable):** A new user reading `docs.agentsfleet.net` finds a Connectors page that names the connect round-trip (click Connect → provider redirect → token vaulted server-side, never pasted) and the per-provider OAuth shape for all five connectors; an operator has a copy-paste runbook for registering the Zoho, Jira, and Linear apps; and `docs/AUTH.md` no longer claims an `api_key` archetype that `docs/architecture/connectors.md` says was dropped.

**Problem:** The connector platform is heavily documented internally (`docs/AUTH.md`, `docs/architecture/connectors.md`) but has **zero user-facing documentation** — a user cannot learn how to connect GitHub or Slack from the docs site. Operators have registration runbooks for Slack and GitHub only; Zoho, Jira, and Linear have none. Two documents have drifted: `AUTH.md` still describes an `api_key` archetype (Datadog/Grafana/Fly) that `connectors.md:52` records as dropped in M108_002, and the GitHub operator playbook seeds a `github-app` bag with the wrong field names.

**Solution summary:** Add one user-facing Connectors guide to the shared docs repo (`~/Projects/docs`) wired into the Fleets nav, cross-linked from the existing secrets and error-code pages. Add three operator playbooks under `playbooks/operations/{zoho,jira,linear}_app_registration/` mirroring the shipped Slack/GitHub runbooks. Correct the two drifts in `docs/AUTH.md` and the GitHub playbook so the connector docs agree with the code (`registry.zig`, `integration_ctx.zig`). No product code changes.

## PR Intent & comprehension handshake

- **PR title (eventual):** docs(m115): connector connect-flow guide, Zoho/Jira/Linear playbooks, drift reconcile
- **Intent (one sentence):** Users can learn the connector connect flow from the docs site, operators can register every OAuth app from a runbook, and the internal connector docs stop contradicting the code.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `playbooks/operations/slack_app_registration/001_playbook.md` — the runbook shape to mirror for Zoho/Jira/Linear: Human-vs-Agent split table, env-resolve §0, browser-interactive app-create steps, vault-write via stdin (RULE VLT), end-to-end verify, Rollback.
2. `docs/AUTH.md` §OAuth connectors (from line 894) — the authoritative behaviour + per-provider flow reference; the `api_key` rows (905, 909, 914) are the stale text to reconcile against `connectors.md`.
3. `docs/architecture/connectors.md` (line 52) — the newer authoritative position: no `api_key` archetype, `REGISTRY.len` pinned at 5. This is the direction the reconcile moves toward, confirmed against `registry.zig`'s pin test before editing AUTH.md.
4. `~/Projects/docs/fleets/secrets.mdx` + `~/Projects/docs/docs.json` — the Mintlify page shape and nav structure the new Connectors guide slots into (Fleets group).
5. `src/agentsfleetd/http/handlers/connectors/integration_ctx.zig` (PlatformSecrets shape) + `registry.zig` — the code source of truth for the `github-app` bag fields (`{app_id, private_key_pem, app_slug}`) and the connector count.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `~/Projects/docs/fleets/connectors.mdx` | CREATE | User-facing connect-flow guide (own docs-repo branch — see Decomposition) |
| `~/Projects/docs/docs.json` | EDIT | Add the Connectors page to the Fleets nav group |
| `~/Projects/docs/fleets/secrets.mdx` | EDIT | Cross-link connectors vs static `${secrets.*}` distinction |
| `playbooks/operations/zoho_app_registration/001_playbook.md` | CREATE | Operator runbook — register Zoho Desk app, seed `zoho-app`, multi-DC redirect |
| `playbooks/operations/jira_app_registration/001_playbook.md` | CREATE | Operator runbook — register Atlassian 3LO app, seed `jira-app` |
| `playbooks/operations/linear_app_registration/001_playbook.md` | CREATE | Operator runbook — register Linear OAuth app, seed `linear-app` |
| `docs/AUTH.md` | EDIT | Remove/annotate the stale `api_key` archetype rows (905, 909, 914) |
| `playbooks/operations/github_app_registration/001_playbook.md` | EDIT | Fix `github-app` bag to `{app_id, private_key_pem, app_slug}` |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — NLG (no new legacy framing), NDC (no dead cross-references left after the AUTH.md edit). No code rules apply — docs/markdown only.
- **`docs/CHANGELOG_VOICE.md`** — the CHORE(close) `changelog.mdx` `<Update>` follows Mintlify voice (routed by `dispatch/write_changelog.md`); the new user guide follows the same load-bearing-facts-kept, no-marketing-words voice.
- **RULE VLT** (vault discipline) — the three new playbooks must write secrets via stdin, never argv, mirroring the Slack playbook §5.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | no | no `*.zig` touched |
| PUB / Struct-Shape | no | no code surface |
| File & Function Length (≤350/≤50/≤70) | no | markdown exempt; playbooks mirror the ~165-line Slack runbook |
| UFS (repeated/semantic literals) | no | no source constants |
| UI Substitution / DESIGN TOKEN | no | no UI components (MDX prose only) |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | no | docs/markdown only; error codes are *referenced*, not defined |

## Prior-Art / Reference Implementations

- **Reference (operator playbooks):** `playbooks/operations/slack_app_registration/001_playbook.md` and `.../github_app_registration/001_playbook.md` — the three new runbooks mirror this structure exactly; Zoho/Jira/Linear are the OAuth-2.0+refresh archetype, so their §5 vault bag is `{client_id, client_secret}` (no `signing_secret`, no private key).
- **Reference (user guide):** `~/Projects/docs/fleets/webhooks.mdx` and `secrets.mdx` — the Mintlify page voice, frontmatter, and cross-link idiom the Connectors guide follows.
- **Divergence:** GitHub is the `app_install` archetype (installation, not code exchange) — its playbook stays the exception; the user guide states this explicitly rather than flattening all five into "OAuth".

## Sections (implementation slices)

### §1 — User-facing Connectors guide

Delivers the missing end-user documentation: a Mintlify page that explains what a connector is, walks the connect round-trip, and tabulates the per-provider OAuth shape. Unblocks a user connecting GitHub/Slack/Zoho/Jira/Linear without reading source. **Implementation default:** slug `fleets/connectors`, title "Connectors" with a one-line note that the dashboard labels the surface "Integrations" (the connector-vs-integration terminology from `connectors.md`), because that matches the existing Fleets-group page naming.

- **Dimension 1.1** — the page names the connect round-trip (Connect → provider redirect → token vaulted server-side, never pasted) and states a fleet needs a human-approved integration grant before it can use the credential → Test `test_connectors_page_flow_prose`
- **Dimension 1.2** — the page tabulates all five providers with correct OAuth shape (GitHub App-install; Slack OAuth2 no-refresh; Zoho/Jira/Linear OAuth2+refresh; Zoho multi-datacenter; Jira cloud-id) → Test `test_connectors_page_five_providers`
- **Dimension 1.3** — the page is wired into `docs.json` Fleets nav and `secrets.mdx` cross-links it (static vendor key = `${secrets.*}`, not a connector) → Test `test_connectors_nav_and_xref`

### §2 — Zoho / Jira / Linear operator playbooks

Delivers the three missing registration runbooks so an operator can seed `zoho-app`/`jira-app`/`linear-app` into the admin vault. Unblocks self-serve operator onboarding for the OAuth-refresh connectors. **Implementation default:** each mirrors the Slack playbook's section shape; the §5 bag is `{client_id, client_secret}` only; Zoho documents the data-center redirect-URL choice, Jira documents that `cloud_id` is resolved at callback (not seeded by the operator).

- **Dimension 2.1** — `zoho_app_registration/001_playbook.md` exists with the Human-vs-Agent split, `zoho-app` stdin vault write, and a resolve-verify step → Test `test_zoho_playbook_shape`
- **Dimension 2.2** — `jira_app_registration/001_playbook.md` exists, seeds `jira-app`, notes Atlassian 3LO + callback-resolved cloud id → Test `test_jira_playbook_shape`
- **Dimension 2.3** — `linear_app_registration/001_playbook.md` exists, seeds `linear-app` → Test `test_linear_playbook_shape`

### §3 — Drift reconcile (AUTH.md + GitHub playbook)

Delivers agreement between the connector docs and the code. Unblocks trust in `AUTH.md` as the behaviour reference. **Implementation default:** confirm `registry.zig`'s pin (len == 5, no `api_key` archetype) first, then edit `AUTH.md` to match `connectors.md` — Datadog/Grafana/Fly documented as plain `agentsfleet secret add` entries, not connectors. Fix the GitHub playbook bag to the code's `{app_id, private_key_pem, app_slug}`.

- **Dimension 3.1** — `AUTH.md` no longer describes an `api_key` connector archetype; Datadog/Grafana/Fly framed as plain workspace secrets, consistent with `connectors.md:52` → Test `test_authmd_no_apikey_archetype`
- **Dimension 3.2** — the GitHub playbook §5 seeds `{app_id, private_key_pem, app_slug}` (no spurious `client_id`/`client_secret`, field names match `integration_ctx.zig`) → Test `test_github_playbook_bag_fields`

## Interfaces

```
No API/code interface. Documentation surfaces only:
- ~/Projects/docs/fleets/connectors.mdx        (new Mintlify page, Fleets nav group)
- playbooks/operations/{zoho,jira,linear}_app_registration/001_playbook.md
Vault bag shapes documented (source of truth = integration_ctx.zig, NOT this spec):
- zoho-app / jira-app / linear-app : { client_id, client_secret }
- github-app                        : { app_id, private_key_pem, app_slug }
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Reconcile in wrong direction | Editing `connectors.md` to re-add `api_key` instead of fixing `AUTH.md` | EXECUTE confirms `registry.zig` pin test (len=5) BEFORE editing; the code is authoritative, AUTH.md yields |
| Playbook secret leak | A new playbook pastes a vendor secret into argv/history | Mirror Slack §5 stdin pattern exactly; gitleaks over the diff must pass; no literal secret values in prose |
| Broken docs build | Malformed MDX frontmatter or a dangling nav entry in `docs.json` | Mintlify build + link check in the rubric; `docs.json` validated as JSON |
| Stale cross-reference | AUTH.md edit leaves a dangling internal link/anchor to removed `api_key` text | NDC grep for orphaned `api_key`/Datadog/Grafana/Fly connector references |

## Invariants

1. Vault bag field names in every new/edited doc match `integration_ctx.zig` — enforced by a grep assertion (Test 3.2), not prose review.
2. `REGISTRY.len == 5` and no `api_key` archetype — the reconcile direction is pinned by `registry.zig`'s own compile-time pin test, re-run at VERIFY.
3. No literal secret material in any playbook — enforced by `gitleaks detect` over the diff.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable | — | documentation-only change; no product or operator signal added | — | — | — |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | verification | `test_connectors_page_flow_prose` | `connectors.mdx` contains "Connect", "vault"/"vaulted", and "grant" — connect round-trip + grant gate described |
| 1.2 | verification | `test_connectors_page_five_providers` | page mentions github, slack, zoho, jira, linear AND "installation"/"App install", "refresh", "cloud" (Jira), "data cent"/"region" (Zoho) |
| 1.3 | verification | `test_connectors_nav_and_xref` | `docs.json` lists `fleets/connectors`; `jq . docs.json` valid; `secrets.mdx` links the connectors page |
| 2.1 | verification | `test_zoho_playbook_shape` | file exists; contains `zoho-app`, a Human-vs-Agent table, a stdin `credential add`, and a multi-datacenter redirect note |
| 2.2 | verification | `test_jira_playbook_shape` | file exists; contains `jira-app`, `auth.atlassian.com`, and a callback-resolves-cloud-id note |
| 2.3 | verification | `test_linear_playbook_shape` | file exists; contains `linear-app`, `linear.app/oauth` |
| 3.1 | verification | `test_authmd_no_apikey_archetype` | `AUTH.md` no longer calls Datadog/Grafana/Fly an `api_key` *archetype*/*connector*; a "not a connector / plain secret" statement is present |
| 3.2 | verification | `test_github_playbook_bag_fields` | github playbook §5 jq object keys == `{app_id, private_key_pem, app_slug}`; no `client_secret` key |
| — | regression | `test_registry_pin_unchanged` | `zig build test` connector registry pin still asserts len=5 — no code drift introduced |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Connectors user guide covers flow + five providers (§1) | `grep -Eic 'github\|slack\|zoho\|jira\|linear' ~/Projects/docs/fleets/connectors.mdx` | ≥5 | P1 | |
| R2 | Guide wired into nav, valid JSON (§1) | `jq -e '.. \| strings \| select(. == "fleets/connectors")' ~/Projects/docs/docs.json` | exit 0 | P1 | |
| R3 | Three operator playbooks exist (§2) | `ls playbooks/operations/{zoho,jira,linear}_app_registration/001_playbook.md` | 3 paths, exit 0 | P1 | |
| R4 | Playbooks seed the correct app bags (§2) | `grep -l -- '-app' playbooks/operations/{zoho,jira,linear}_app_registration/001_playbook.md \| wc -l` | 3 | P1 | |
| R5 | AUTH.md api_key drift reconciled (§3) | `grep -in 'api_key.*archetype\|are the \*\*api_key\*\*' docs/AUTH.md` | 0 matches | P1 | |
| R6 | GitHub playbook bag matches code (§3) | `grep -n 'private_key_pem\|app_slug' playbooks/operations/github_app_registration/001_playbook.md` | ≥1 match | P1 | |
| R7 | Diff stays inside Files Changed | `git diff --name-only origin/main` (both repos) | 0 paths missing from the Files Changed table | P0 | |
| S1 | No secrets in the diff | `gitleaks detect` | exit 0 | P0 | |
| S2 | Registry pin unchanged (no code drift) | `zig build test` (connector registry pin) | exit 0 | P0 | |
| S3 | Docs build / link check | Mintlify build in `~/Projects/docs` (`mint dev`/CI link check) | exit 0 / no broken links | P1 | |
| S4 | Orphan sweep — no dangling api_key connector refs | Dead Code Sweep greps | 0 matches | P1 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line (`342 passed`); long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE; a P1 ❌ ships only with an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

| File to delete | Verify |
|----------------|--------|
| N/A — no files deleted | — |

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `api_key` connector archetype (Datadog/Grafana/Fly framed as connectors) | `grep -rn 'api_key' docs/AUTH.md \| grep -i 'archetype\|connector'` | 0 matches |

## Out of Scope

- **CLI connector surface** — none exists (`connectors.md`); documenting a non-existent CLI is out of scope.
- **Per-provider deep-dive pages** (one page each) — a single Connectors guide is sufficient for now; split later if a provider grows quirks.
- **`api_key` connector documentation** — the archetype was dropped (M108_002); Datadog/Grafana/Fly are plain secrets, documented as such, not as connectors.
- **Any code change** to the connector platform — this workstream documents shipped behaviour only.

---

## Product Clarity (authoring record)

1. **Successful user moment** — A user lands on the Connectors page, reads that connecting is a redirect (never a token paste), sees their provider in the five-row table with its exact OAuth shape, and knows their fleet needs a grant before it can act — no source-reading required.
2. **Preserved user behaviour** — Existing Fleets docs (secrets, webhooks, authoring) keep working unchanged; the nav gains one entry, reorders nothing.
3. **Optimal-way check** — Yes: one guide page + three runbooks + two drift fixes is the most direct close of the audited gaps; no gap needs more surface than that.
4. **Rebuild-vs-iterate** — Iterate. Purely additive docs plus two corrections; no refactor, and nothing here trades away determinism.
5. **What we build** — 1 user MDX page + nav entry + 1 cross-link; 3 operator playbooks; 2 drift fixes (AUTH.md, GitHub playbook).
6. **What we do NOT build** — CLI connector docs (no CLI), per-provider pages (one guide suffices), api_key connector docs (archetype dropped).
7. **Fit with existing features** — Compounds with the secrets and webhooks docs and the operator playbook set; must not destabilize `connectors.md` as the architecture source of truth (the guide summarizes, never forks it).
8. **Surface order** — Both: user-facing guide (docs site) and operator-facing runbooks (repo playbooks); the audit found both missing.
9. **Dashboard restraint** — N/A — no UI built.
10. **Confused-user next step** — The Connectors guide itself plus the `UZ-CONN-*` / `UZ-SLK-*` error-code page (already published) is the self-serve path; no ticket required.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** One workstream, three Sections split by surface (user docs / operator playbooks / drift reconcile) — each independently verifiable and reviewable, all sharing the "document the shipped connector platform" intent.
- **Alternatives considered:** (a) Fold the drift fixes into a separate tiny spec — rejected: they are connector-doc corrections, same blast radius, cheaper as one PR. (b) One deep-dive page per provider — rejected as premature surface (Out of Scope).
- **Patch-vs-refactor verdict:** this is a **patch** — additive documentation plus two targeted corrections; no architecture moves. **Two-repo delivery:** `~/Projects/docs` edits land on their own `chore/m115-*` branch and PR per the docs-repo-own-branch rule; the `agentsfleet` repo carries the playbooks, AUTH.md fix, spec, and changelog. Both PRs reference this spec.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage: {empty at creation}
- **Metrics review** — {empty at creation — expected: "no analytics/funnel playbook update required, documentation-only"}
- **Skill-chain outcomes** — `/write-unit-test`, `/review`, `kishore-babysit-prs`: {empty at creation}
- **Deferrals** — {empty at creation}
