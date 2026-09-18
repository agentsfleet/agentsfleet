<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the orly-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M199_001: CLI completes install → approve → steer without leaving the terminal

**Prototype:** v2.0.0
**Milestone:** M199
**Workstream:** 001
**Date:** Sep 18, 2026
**Status:** IN_PROGRESS
**Priority:** P0 — a fleet whose trigger declares `repository_access: write` parks on an approval gate the CLI cannot see, so `steer` can never complete from the terminal.
**Categories:** CLI, DOCS
**Batch:** B1 — single stream; no parallel workstream depends on it.
**Branch:** feat/m199-cli-library-approvals-consistency
**Baseline revision:** 4079743cb7f760b2b4ae45c877efbf062c9a7618
**Test Baseline:** unit=1673 — Command-Line Interface (CLI) 1673 passed / 14 skipped / 0 failed (`cd cli && bun test`) at `4079743cb`, measured in a detached worktree with `dist/` built. The diff is TypeScript under `cli/` alone, so the Rust half of `make test-unit-all` (2630 passed / 0 failed) and the integration lane are identical by construction and reported once, from the branch. Branch: 1748 passed / 16 skipped / 0 failed — delta +75.
**Baseline evidence:** `playbooks/operations/acceptance/baselines/M199_001-4079743cb.md`
**Depends on:** none
**Provenance:** agent-generated (pre-spec, live probe of https://api-dev.agentsfleet.net on Sep 18, 2026)
**Canonical architecture:** `docs/architecture/fleet_library.md` §1

---

## Overview

**Goal (testable):** `agentsfleet library add --github <owner/repo>` and `--from <path>` create a tenant library the same `agentsfleet library` run then lists, and `agentsfleet approvals approve <gate_id>` releases a gate so a previously parked `agentsfleet steer` completes — all without curl, the dashboard, or an environment variable.

**Problem:** Three server surfaces have no command. An operator who installs a Fleet watches `steer` sit for sixty seconds and then fail with a timeout, because the message is parked behind an approval gate; `agentsfleet status` still says `active`, `agentsfleet events` shows only `received`, and nothing in the terminal names the gate or offers to release it. Separately, a Fleet library onboarded into a workspace is installable by identifier but invisible to `agentsfleet library`, which reads the platform-only catalogue — so the identifier `install` needs can only come from the dashboard, and the CLI's own error text says so.

**Solution summary:** `library` moves onto the workspace gallery so it lists what `install` resolves against, gaining the tier and source columns that tell a platform row from a tenant one. A new `library add` posts the onboarding body the daemon already accepts, in its three source kinds. A new `approvals` group lists, shows, approves, and denies gates. `steer` and `status` stop reporting a parked Fleet as a bare timeout: both read the pending-approval count the Fleet detail already returns and name the command that clears it. A consistency pass makes the placeholder spelling, the `secret list` rendering, and the duplicated steer failure line agree with the rest of the surface.

## PR Intent & comprehension handshake

- **PR title (eventual):** feat(cli): library onboarding, approvals inbox, and parked-fleet diagnosis
- **Intent (one sentence):** An operator can create a Fleet library, install it, release the approval gate it parks on, and get a steer response, using only the Command-Line Interface (CLI).
- **Handshake** — pending until the implementing agent performs PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `cli/src/commands/fleet_install.ts` — the install flow already resolves the workspace gallery, keys a create body off `visibility`, and renders declared requirements; `library` and `library add` mirror its request shape and its requirement preview.
2. `cli/src/commands/fleet_library.ts` — the command being repointed; its table, empty-state, and JavaScript Object Notation (JSON) branch are the shape the new columns extend.
3. `rustd/crates/afd_http/src/handler/library_onboard.rs` — the parse both onboarding planes share; its refusal constants are the exact failure set `library add` must surface.
4. `rustd/crates/afd_api_tenant/src/handler/approval/resolve.rs` — the decision route (`approve` / `deny`) and the sentence it returns for an unknown decision.
5. `cli/src/program/cli-tree-fleet.ts` — how a command group declares options, metavars, and an epilogue; new groups follow it rather than inventing a second registration style.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `cli/src/commands/fleet_library.ts` | EDIT | Reads the workspace gallery; gains tier and source columns. |
| `cli/src/commands/fleet_library_add.ts` | CREATE | Shapes and posts the onboarding body for the three source kinds. |
| `cli/src/commands/approvals.ts`, `cli/src/commands/approvals_decide.ts` | CREATE | List and show gates; post the approve and deny decisions. |
| `cli/src/commands/approvals_pending.ts` | CREATE | The pending-gate lookup `steer` and `status` both read. |
| `cli/src/commands/fleet_steer.ts` | EDIT | One failure line; names the gate holding a parked run. |
| `cli/src/commands/fleet.ts` | EDIT | Status renders the waiting count and the command that clears it. |
| `cli/src/commands/fleet_list.ts` | EDIT | The next-page hint named a command that does not exist. |
| `cli/src/commands/fleet_secret.ts` → `cli/src/commands/fleet_secret_list.ts` | EDIT, CREATE | The vault list moves out to stay inside the length cap, and becomes a table with ISO 8601 timestamps. |
| `cli/src/commands/fleet_install_source.ts` | EDIT | Reads the shared library-identifier placeholder. |
| `cli/src/commands/auth-logout.ts`, `cli/src/program/cli-tree.ts` | EDIT | Logout stops claiming a revocation scope the daemon does not deliver. |
| `cli/src/lib/api-paths.ts` | EDIT | Adds the approvals paths. |
| `cli/src/lib/http.ts`, `cli/src/services/http-client.ts` | EDIT | Parse and render the daemon's `user_message` instead of its log-shaped `detail`. |
| `cli/src/constants/approvals.ts`, `cli/src/constants/library-source.ts` | CREATE | Decision, status, column, source-kind, bundle-file, and tier literals. |
| `cli/src/constants/cli-flags.ts` | EDIT | One spelling for the library identifier and the new option keys. |
| `cli/src/program/cli-tree-fleet.ts`, `cli/src/program/cli-tree-access.ts` | EDIT | Register `library add` and the `approvals` group; correct the library placeholder. |
| `cli/src/program/cli-tree-types.ts`, `cli/src/program/handlers-bind-fleet.ts`, `cli/src/program/handlers-bind-access.ts` | EDIT | Extend the handler record and bind the new commands. |
| `cli/test/approvals.integration.test.ts` | CREATE | Approvals against the mock Application Programming Interface (API) layer. |
| `cli/test/approvals-pending.unit.test.ts` | CREATE | The pending-gate lookup, including its defect-safety. |
| `cli/test/library-add.integration.test.ts` | CREATE | Onboarding and every client-side refusal. |
| `cli/test/secret-list-render.integration.test.ts` | CREATE | Vault rendering, secrecy, and the parked-Fleet status. |
| `cli/test/cli-consistency.unit.test.ts`, `cli/test/error-rendering.unit.test.ts` | CREATE | One spelling per concept; every hint names a real command; which refusal string reaches the terminal. |
| `cli/test/acceptance/library-onboard-live.spec.ts` | CREATE | End-to-end onboarding and listing through the built binary. |
| `cli/test/acceptance/approvals-live.spec.ts` | CREATE | End-to-end gate decision through the built binary. |
| `cli/test/acceptance/grant-approval-live.spec.ts`, `cli/test/acceptance/fixtures/grant-ops.ts`, `cli/test/acceptance/run-lane.ts` | EDIT | The card is answered through the CLI; the HTTP helper it replaced goes with it; the lane registers the two new specs. |
| `cli/test/fleet-library.unit.test.ts` | EDIT | Gallery path, tier column, and the new empty state. |
| `cli/test/fleet-steer-errors.integration.test.ts`, `cli/test/fleet-steer-linecov.unit.test.ts` | EDIT | The stall is reported once, on the failure. |
| `cli/test/fleet.integration.test.ts`, `cli/test/cli-alignment.unit.test.ts` | EDIT | Status now also reads the approvals inbox; the corrected next-page hint. |
| `cli/test/fleet-install-unit.test.ts`, `cli/test/cli-tree.fleet.unit.test.ts`, `cli/test/acceptance/options-metavar.spec.ts`, `cli/test/golden/help-no-color.txt` | EDIT | The corrected library placeholder and help bodies. |
| `cli/test/fleet-secret-errors.unit.test.ts` | EDIT | Imports the relocated vault list. |
| `cli/test/command-matrix-parity.unit.test.ts`, `cli/test/acceptance/fixtures/command-matrix.ts` | EDIT | Approvals rows, and a group that owns subcommands and also runs. |
| `cli/test/helpers-cli-tree.ts`, `cli/test/json-contract.test.ts` | EDIT | Handler stubs for the new commands. |
| `docs/v2/active/M199_001_P0_CLI_DOCS_LIBRARY_ONBOARDING_AND_APPROVAL_INBOX.md`, `playbooks/operations/acceptance/baselines/M199_001-4079743cb.md` | CREATE | This spec and the baseline evidence its header names. |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — UFS (every decision literal, source kind, gate kind and column label is a named constant, never an inline string), NDC (no command registered without a bound handler and a test), NLR (the `secret list` renderer and the duplicated steer failure line are fixed where touched), NLG (no alias kept for the platform-only `library` behaviour), FLL (each new command file stays inside the length caps; `fleet_library.ts` splits rather than absorbing the add path), ORP (the platform-catalogue constant is swept if it loses its last reader), TSC and TSJ (TypeScript conventions for the new modules), TST-NAM (test names carry no milestone identifier), MSID (no `M199` or section marker in source).
- `dispatch/write_ts_adhere_bun.md` — every file in this diff is TypeScript; the TypeScript File Shape Decision is made at PLAN.
- `dispatch/write_any.md` — length, logging, milestone-identifier and named-constant gates fire on every source file here.
- `docs/DOCUMENTATION_RULES.md` — the `~/Projects/docs` pages this changes are published prose.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| UI GATE, DESIGN TOKEN GATE, SCHEMA GUARD, ZIG GATE, PUB GATE | no — the diff carries no `ui/`, stylesheet, `schema/*.sql`, `*.zig`, or Rust file | N/A |
| UFS GATE | yes — new literals for decisions, source kinds, and columns | Each set lands in `cli/src/constants/`, imported by both the command and its test. |
| LENGTH GATE | yes — `fleet_library.ts` and `cli-tree-fleet.ts` both grow | The add path lands in its own module; the approvals group splits list/show from decide. |
| MILESTONE-ID GATE | yes — every source file is in scope | No milestone identifier appears in `cli/src/**`; the spec carries the identity. |
| LOGGING GATE, GREPTILE GATE | yes | Failures ride the existing `CliError` taxonomy, no bare `console` in a handler; the rule identifiers named above are obeyed by construction. |
| File & Function Length (≤350/≤50/≤70) | yes | Command modules stay single-purpose; rendering splits from request building. |

## Prior-Art / Reference Implementations

- **Reference:** `cli/src/commands/fleet_install.ts` — the closest shipped pattern: resolves the same gallery, previews the same requirements, and shapes a create body from a resolved tier. `library add` mirrors its request and preview; it diverges only in posting rather than resolving.
- **Reference:** `cli/src/commands/grant.ts` — the existing workspace-scoped list-plus-mutate group; the `approvals` group mirrors its table, its identifier argument, and its error shape.
- **7 Pillars alignment** — command → handler → errors split (new commands are Effects with no input/output of their own), handler purity (no direct writes to standard output inside a handler; rendering goes through the `Output` service), output as a service (human table and JSON branch chosen by the renderer), structured errors with a suggestion line (every new failure carries `detail` + `suggestion` on the existing `CliError`), and the three-tier test pyramid (unit, mock-API integration, subprocess acceptance). Divergence: automatic JSON when standard output is piped is not adopted here, because the repository's existing commands all key off the explicit `--json` flag and changing that is a separate decision.

## Sections (implementation slices)

### §1 — `library` lists what `install` resolves

`agentsfleet library` reads the workspace gallery, so the identifier a user copies is one `install` accepts, and a tenant library stops being invisible. Non-obvious choice → **Implementation default:** `read the workspace gallery only` because a union of two endpoints would have to reconcile two row shapes and could disagree with `install`, which is the bug being fixed.

- **Dimension 1.1** — `library` requests the workspace gallery path and renders one row per returned entry → Test `test_library_lists_workspace_gallery` — DONE
- **Dimension 1.2** — each row shows its tier, so a platform entry and a tenant entry sharing a name are distinguishable → Test `test_library_row_shows_tier` — DONE
- **Dimension 1.3** — the empty state names `library add` as the next move rather than only `install` → Test `test_library_empty_state_names_add` — DONE
- **Dimension 1.4** — the JSON branch emits the same entries the table renders → Test `test_library_json_matches_table` — DONE
- **Dimension 1.5** — `install --library` keeps resolving a platform entry exactly as before → Test `test_install_library_flag_unchanged` — DONE

### §2 — `library add` onboards a bundle

A workspace gains a Fleet library from a public repository, a local bundle directory, or a first-party template, using the onboarding body the daemon already parses. Non-obvious choice → **Implementation default:** `exactly one of --github / --from / --template is required` because the daemon's parse refuses mixed shapes and a client-side refusal costs no request.

- **Dimension 2.1** — `--github <owner/repo>` posts a github source and reports the created identifier → Test `test_library_add_github_posts_source` — DONE
- **Dimension 2.2** — `--ref <revision>` rides the github source and is refused on the other two kinds → Test `test_library_add_ref_rejected_off_github` — DONE
- **Dimension 2.3** — `--from <path>` reads the bundle's skill and trigger documents and posts an upload source → Test `test_library_add_upload_reads_bundle` — DONE
- **Dimension 2.4** — `--template <id>` posts a template source → Test `test_library_add_template_posts_source` — DONE
- **Dimension 2.5** — zero or more than one source flag is refused before any request leaves the process → Test `test_library_add_requires_exactly_one_source` — DONE
- **Dimension 2.6** — a daemon refusal renders its `user_message` and its documentation link rather than a bare status code → Test `test_library_add_renders_server_refusal` — DONE

### §3 — The approvals inbox

A gate is visible, inspectable, and decidable from the terminal. Non-obvious choice → **Implementation default:** `approve and deny are separate subcommands, not a --decision flag` because the daemon models the decision as a path segment with its own capability, and mirroring that keeps the two audit-distinguishable.

- **Dimension 3.1** — `approvals list` renders every gate with its kind, status, and the Fleet it belongs to → Test `test_approvals_list_renders_gates` — DONE
- **Dimension 3.2** — `--fleet <id>` narrows the list to one Fleet → Test `test_approvals_list_filters_by_fleet` — DONE
- **Dimension 3.3** — `approvals show <gate_id>` renders the proposed action and the blast radius in full → Test `test_approvals_show_renders_blast_radius` — DONE
- **Dimension 3.4** — `approvals approve <gate_id>` posts the approve decision and reports the outcome → Test `test_approvals_approve_posts_decision` — DONE
- **Dimension 3.5** — `approvals deny <gate_id>` posts the deny decision → Test `test_approvals_deny_posts_decision` — DONE
- **Dimension 3.6** — an already-resolved gate reports its existing outcome rather than a generic failure → Test `test_approvals_resolved_gate_reports_outcome` — DONE
- **Dimension 3.7** — a second decision on a decided gate is not reported as a fresh one → Test `test_approve_twice_is_not_a_second_decision` — DONE

### §4 — A parked Fleet says it is parked

`steer` and `status` name the gate that is holding a Fleet, so the sixty-second timeout stops being the only signal. Non-obvious choice → **Implementation default:** `read the pending count from the Fleet detail already fetched` because it needs no extra request on the happy path.

- **Dimension 4.1** — `status` shows the pending-approval count for a Fleet that has one → Test `test_status_shows_pending_approvals` — DONE
- **Dimension 4.2** — a `steer` that times out with a pending gate names the gate and the command that clears it → Test `test_steer_timeout_names_pending_gate` — DONE
- **Dimension 4.3** — a `steer` that times out with no pending gate keeps its current wording → Test `test_steer_timeout_without_gate_unchanged` — DONE
- **Dimension 4.4** — the timeout failure prints one failure line, not two → Test `test_steer_timeout_prints_single_failure` — DONE

### §5 — Consistency pass

The surface stops contradicting itself where the contradiction is text or rendering rather than a flag rename. Non-obvious choice → **Implementation default:** `rename no flag and change no JSON key in this spec` because both break a scripted caller and deserve a diff whose only subject is that breakage.

- **Dimension 5.1** — one spelling per concept: the library identifier placeholder reads the same in the option, the epilogue, and the empty state, and every paging option sharing a concept carries one description → Test `test_option_text_spelling_agrees` — DONE
- **Dimension 5.2** — `secret list` renders a header and aligned columns like every other list → Test `test_secret_list_renders_table` — DONE
- **Dimension 5.3** — `secret list` renders its timestamp in ISO 8601, matching `api-key list` → Test `test_secret_list_timestamp_is_iso` — DONE
- **Dimension 5.5** — the `secret list --json` key set is unchanged → Test `test_secret_list_json_keys_unchanged` — DONE

## Interfaces

```
GET  /v1/workspaces/{ws}/fleet-libraries
     -> { items: [ { id, name, description, visibility, source_ref, created_at,
           requirements: { credentials[], tools[], network_hosts[], trigger_present } } ] }
POST /v1/workspaces/{ws}/fleet-libraries
     <- { source_kind: "github"|"upload"|"template", source_ref, ref?, replace,
          skill_markdown?, trigger_markdown?, support_files: [] }
     -> 201 { id, name, visibility, content_hash, requirements }
     -> 400 { error_code, title, detail, user_message, docs_uri, request_id }
GET  /v1/workspaces/{ws}/approvals            (and /{gate_id} for one)
     -> { items: [ { gate_id, fleet_id, fleet_name, gate_kind, tool_name, status,
           proposed_action, blast_radius, created_at, timeout_at, resolved_by } ] }
POST /v1/workspaces/{ws}/approvals/{gate_id}/{approve|deny}
     -> 200 { gate_id, action_id, outcome, resolved_at, resolved_by }

Command surface added:
  agentsfleet library add (--github <owner/repo> [--ref <rev>] | --from <path> | --template <id>) [--replace]
  agentsfleet approvals list [--fleet <id>] | show <gate_id> | approve <gate_id> | deny <gate_id>
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Wrong source-flag count | `library add` bare, or `--github` with `--from` | Refused before any request; exit 4; the suggestion names the three flags and the exclusion. |
| Revision off github | `--ref` with `--from` or `--template` | Refused before any request; exit 4; the suggestion says a revision belongs to a github source. |
| Daemon refuses the bundle | Unparseable trigger frontmatter, oversized file, embedded credential | The daemon's `user_message` and documentation link are rendered; exit 3. |
| Unknown library identifier | `install --library` names an entry the gallery lacks | Existing behaviour retained; the suggestion now names `library` alone, because the gallery is the one place identifiers come from. |
| Gate already resolved, or unknown | `approvals approve` on a decided gate; `approvals show` on an unknown one | The daemon's own refusal sentence is rendered; a decided gate reports the outcome that stands; exit 3. |
| Steer parked behind a gate | Fleet has a pending gate when the message times out | One failure line naming the gate count and `agentsfleet approvals list`; exit 3. |

## Invariants

1. Exactly one source kind rides an onboarding request — enforced by a single parse in the command that returns a tagged result, so no downstream branch can read a second source.
2. The identifier `library` prints is the identifier `install --library` accepts — enforced by both reading one path constant in `cli/src/lib/api-paths.ts`, with a test that asserts the two commands request the same path.
3. A decision literal appears once in the source — enforced by the named-constant module plus the repository's const-name audit script, which the lint lane runs.
4. No approval decision is inferred — the command posts only the decision the operator named as a subcommand; there is no default and no prompt-driven fallback.
5. A secret's bytes never reach standard output — the vault renderer reads only `name`, `created_at`, and `kind`, and the existing secrecy test asserts no other field is printed.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `cli_command_invoked` | product | Any new subcommand runs, through the existing command-invocation telemetry | command path, exit class, duration | No bundle content, no gate blast-radius text, no secret name or value | `test_new_commands_emit_invocation_event` |

The repository's existing analytics already records command invocation; this spec adds command paths to that stream and renames nothing. No funnel changes, so no analytics playbook update is required.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `test_library_lists_workspace_gallery` | A stubbed gallery of two entries renders two rows; the requested path is the workspace gallery, not the platform catalogue. |
| 1.2 | unit | `test_library_row_shows_tier` | Two entries named alike, one platform and one tenant, render distinguishable rows. |
| 1.3 | unit | `test_library_empty_state_names_add` | An empty gallery prints a line containing `library add`. |
| 1.4 | unit | `test_library_json_matches_table` | The JSON branch and the table branch carry the same identifiers for one stubbed response. |
| 2.1 | integration | `test_library_add_github_posts_source` | `--github owner/repo` posts `source_kind: "github"` and `source_ref: "owner/repo"`; the created identifier is printed. |
| 2.2 | unit | `test_library_add_ref_rejected_off_github` | `--ref v1 --template x` exits 4 without a request; `--ref v1 --github o/r` posts `ref: "v1"`. |
| 2.3 | integration | `test_library_add_upload_reads_bundle` | A fixture directory posts `source_kind: "upload"` carrying both documents' bytes. |
| 2.4 | integration | `test_library_add_template_posts_source` | `--template github-pr-reviewer` posts `source_kind: "template"`. |
| 2.5 | unit | `test_library_add_requires_exactly_one_source` | Bare invocation and two-flag invocation both exit 4 with no request issued. |
| 2.6 | integration | `test_library_add_renders_server_refusal` | A stubbed 400 carrying `user_message` prints that sentence and the documentation link; exit 3. |
| 3.1 | unit | `test_approvals_list_renders_gates` | Two stubbed gates render two rows carrying kind and status. |
| 3.2 | integration | `test_approvals_list_filters_by_fleet` | `--fleet <id>` renders only that Fleet's gates. |
| 3.3 | unit | `test_approvals_show_renders_blast_radius` | The full blast-radius sentence is printed untruncated. |
| 3.4 | integration | `test_approvals_approve_posts_decision` | The request path ends in `/approve`; the printed outcome is the response's `outcome`. |
| 3.5 | integration | `test_approvals_deny_posts_decision` | The request path ends in `/deny`. |
| 3.6 | integration | `test_approvals_resolved_gate_reports_outcome` | A stubbed conflict refusal prints the existing outcome; exit 3. |
| 4.1 | unit | `test_status_shows_pending_approvals` | A Fleet detail carrying a non-zero pending count renders that count. |
| 4.2 | unit | `test_steer_timeout_names_pending_gate` | A timeout with a pending gate prints a line containing `approvals`. |
| 4.3 | unit | `test_steer_timeout_without_gate_unchanged` | A timeout with no pending gate prints the existing sentence. |
| 4.4 | unit | `test_steer_timeout_prints_single_failure` | Exactly one line begins with the failure glyph. |
| 5.1 | unit | `test_option_text_spelling_agrees` | The option metavar, epilogue, and empty state carry one library-identifier spelling; paging options sharing a concept carry one description. |
| 5.2 | unit | `test_secret_list_renders_table` | Output carries a header row and a rule line, like `api-key list`. |
| 5.3 | unit | `test_secret_list_timestamp_is_iso` | No bare epoch-millisecond integer appears; timestamps match the ISO 8601 shape. |
| 2.1 | e2e | `library-onboard-live` | Through the built binary against a live daemon: `library add --github`, then `library` lists the created entry. |
| 3.4 | e2e | `approvals-live` | Through the built binary: `approvals list` shows a pending gate, `approvals approve` clears it, and the Fleet's pending count falls. |
| 1.5 | unit | `test_install_library_flag_unchanged` | `install --library <id>` still resolves and installs a platform entry exactly as before. |
| 5.5 | unit | `test_secret_list_json_keys_unchanged` | The `secret list --json` key set is byte-identical to the current output. |
| 3.7 | integration | `test_approve_twice_is_not_a_second_decision` | A second approve on the same gate does not report a fresh decision. |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | `library` lists a tenant entry `install` accepts (§1) | `node cli/dist/bin/agentsfleet.js library --api https://api-dev.agentsfleet.net` | output contains a `tenant` tier row | P0 | |
| R2 | `library add --github` creates an entry (§2) | `node cli/dist/bin/agentsfleet.js library add --github agentsfleet/github-pr-reviewer --api https://api-dev.agentsfleet.net` | exit 0 and an identifier is printed | P0 | |
| R3 | `library add --from` uploads a local bundle (§2) | `node cli/dist/bin/agentsfleet.js library add --from tests/fixtures/fleetbundle/github-pr-reviewer --api https://api-dev.agentsfleet.net` | exit 0 and an identifier is printed | P0 | |
| R4 | A gate is decidable from the terminal (§3) | `node cli/dist/bin/agentsfleet.js approvals list --api https://api-dev.agentsfleet.net` | exit 0; a pending gate renders with its kind | P0 | |
| R5 | A parked steer names its gate (§4) | `node cli/dist/bin/agentsfleet.js steer <parked_fleet> "ping" --api https://api-dev.agentsfleet.net` | failure text contains `approvals` and exactly one failure glyph line | P0 | |
| R6 | `secret list` renders like every other list (§5) | `node cli/dist/bin/agentsfleet.js secret list --api https://api-dev.agentsfleet.net` | output carries a header row; no bare epoch integer | P1 | |
| R7 | Diff stays inside Files Changed | `git diff --name-only origin/main...HEAD` | 0 paths missing from the Files Changed table | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3a | Lint green | `make lint-all` | exit 0 | P0 | |
| S3b | Integration lane green | `make test-integration-rustd` | exit 0 | P0 | |
| S3c | Version sync green | `make check-version` | exit 0 | P0 | |
| S4 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S5 | No oversize source file | `git diff --name-only origin/main...HEAD \| grep -v '\.md$' \| xargs wc -l 2>/dev/null \| awk '$1>350 && $2!="total"'` | no output | P0 | |
| S6 | Orphan sweep | Dead Code Sweep greps | 0 matches | P0 | |

**Command source rule:** every declared `conform` and `verify.*` invocation from `.oracle/orly.json` appears above verbatim with an Expected value. Timing follows `dispatch/lifecycle.md`; baseline metadata is pending at opening and measured before the Pull Request.

**Grading protocol (VERIFY):** run each Verify command verbatim; Graded = ✅/❌ + one decisive output line. Repository-command rows point at the final `orly gate pr` results in Session Notes. **Ship gate:** any ❌ or missing evidence returns to EXECUTE; a P1 ❌ requires an Indy-acked deferral quote in Discovery.

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.** N/A — no files deleted.

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `FLEET_BUNDLES_PATH` | `grep -rn "FLEET_BUNDLES_PATH" cli/src/ \| head` | 0 matches if `library` was its last reader; a surviving reader is named in Discovery |

## Out of Scope

- **Renaming `--workspace-id` to `--workspace`, and `--cursor` to `--starting-after`.** Both spellings ship today across different command groups. Renaming either breaks a scripted caller, and the repository forbids keeping an alias, so the break deserves a Pull Request whose only subject is that break and its migration note. Follow-up spec: M200_001.
- **Changing `connector list --json` from a bare array to an `{ items: [...] }` envelope.** Same reason: a machine-surface break with its own blast radius.
- **Renaming `schedule rm` to `schedule delete`.** Same class.
- **Deduplicating provider aliases in `agentsfleet models`.** The duplicate rows come from the daemon's catalogue; a client-side filter would hide a real catalogue defect.
- **Surfacing a precise refusal reason for an invalid bundle, and repairing `tests/fixtures/fleetbundle/platform-ops/TRIGGER.md` plus the `agentsfleet/platform-ops` repository, whose trigger frontmatter is not parseable YAML Ain't Markup Language (YAML).** Both are daemon-side or fixture-side; neither is read by any test this spec adds. Named in Discovery.

---

## Product Clarity (authoring record)

1. **Successful user moment** — An operator runs `agentsfleet steer`, sees it stop and say a gate is waiting, runs `agentsfleet approvals approve <id>`, re-runs steer, and reads the Fleet's reply. The terminal never sent them to a browser.
2. **Preserved user behaviour** — `install --library <id>` keeps working on platform identifiers; every existing flag keeps its spelling; `secret list --json` keeps its key set; the `--api` flag stays the way a caller points at a non-default daemon.
3. **Optimal-way check** — The most direct shape would have the daemon carry the blocking gate inside the steer failure, needing no second command. That is a daemon change; reading the pending count the Fleet detail already returns buys the same moment and leaves the daemon untouched.
4. **Rebuild-vs-iterate** — Iterate. Command tree, Effect services, renderer, and error taxonomy are sound; what is missing is three commands and one diagnosis.
5. **What we build** — `library` repointed at the gallery; `library add`; the `approvals` group; the parked-Fleet diagnosis in `steer` and `status`; the vault renderer and placeholder corrections.
6. **What we do NOT build** — Flag renames, JavaScript Object Notation envelope changes, an interactive approvals picker, bundle authoring or scaffolding, daemon-side refusal detail, and provider-alias deduplication. Each is either a machine-surface break that wants its own Pull Request or work on a different component.
7. **Fit with existing features** — Compounds with install and steer, which are the two commands an operator already runs most. It must not destabilise install: that path resolves the same gallery, and its behaviour on platform identifiers is pinned by a regression test.
8. **Surface order** — Command-Line Interface first, which is the repository default and, here, the whole point: the dashboard already has all three surfaces.
9. **Dashboard restraint** — N/A — no user interface surface changes.
10. **Confused-user next step** — Every new failure carries a suggestion line naming a command to run. A parked steer names `agentsfleet approvals list`; an empty gallery names `agentsfleet library add`; a rejected source-flag combination names the three flags.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** Five Sections split by the surface each one repairs: what `library` reads, what it can create, what the approvals group adds, what a parked Fleet reports, and what the consistency pass corrects. The split is acyclic — §2 and §4 both benefit from §1's path constant but neither requires the other — so a Section can land and be graded on its own.
- **Alternatives considered:** Moving every command onto a client generated from the daemon's OpenAPI document — rejected for now: it touches the whole tree, and the defect is three missing commands, not a drifting client. Adding only `approvals` — rejected, because it leaves the library identifier reachable only from the dashboard, which is half the reported problem.
- **Patch-vs-refactor verdict:** this is a **patch** because the existing command, handler, and rendering shapes are the right ones and are being extended, not replaced. The one structural change — `library` changing which endpoint it reads — is the bug fix itself.

## Discovery (consult log)

- **Consults** — Architecture: `docs/architecture/fleet_library.md` read for the tier model before naming the gallery columns; the platform-versus-tenant split is that document's, not this spec's. Live probe of `https://api-dev.agentsfleet.net` on Sep 18, 2026 established, with responses recorded in the Pull Request Session Notes: the workspace gallery returns both tiers while `agentsfleet library` renders one; onboarding succeeds for github and upload sources; a `repository_write` gate is created by a steer and blocks it until decided; the Fleet detail returns a non-zero pending-approval count while `status` renders `active`.
- **Source findings not repaired here** — `tests/fixtures/fleetbundle/platform-ops/TRIGGER.md` line 12 carries `context_cap_tokens: {{context_cap_tokens}}` unquoted, which is not parseable YAML Ain't Markup Language (YAML); the same bytes ship in the `agentsfleet/platform-ops` repository, so onboarding that repository fails. The daemon refuses it with a sentence naming a missing skill document and oversized files, neither of which is the cause, because the precise reason is computed and discarded before the response. Both are named in Out of Scope.
- **Metrics review** — pending `/review`.
- **Skill-chain outcomes** — pending: `/orly-write-unit-test`, `/review`, `orly-babysit-prs`.
- **Deferrals** — none. Every item not built is scoped out at authoring with its reason, not deferred.
