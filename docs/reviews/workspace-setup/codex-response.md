---
type: explanation
audience: contributor
verified: 2026-10-14
product_version: 0.60.0
executable: false
---

# Codex resolution of the October14 reviews

## What it is

The revised response to Chief Technology Officer (CTO) AIWA reviews from Claude and ChatGPT. Verdict: agree with the backend rethink and owner decisions, retain the approved interface, and implement through one cutover PR. This replaces earlier alternatives as the live recommendation.

## Why it exists

The previous reconciliation kept unnecessary resources and machinery. The revised design wraps existing typed policy, uses private message recovery, keeps setup progress on installing fleets, and reduces endpoints. Review disagreement must expose evidence and the cost of the chosen option.

## How it behaves

### Agreement and rebuttal ledger

| Topic | Resolution and evidence | Cost / qualification |
|---|---|---|
| Definition | Agree: existing typed config plus instructions/schema envelope; `afd_fleet_runtime/src/config/raw/document.rs:42–98`. | L20 changes all money readers; no claim of migration-free work. |
| Setup worker | Agree: bounded request steps, retained installing identity, explicit retry. `afd_fleet_lifecycle/src/install.rs:223,263–271` lacks that proof. | Abandoned setup waits for Resume; broad claims that the repository has no sweeps are false. |
| Cancel authority | Agree: existing Kill under fleet:write; no creator-only setup cancel. | Any authorized fleet writer can cancel a teammate's setup, as with existing Kill. Delete remains fleet:admin. |
| Preview | Agree: GET draft supplies review; aliases are deferred. | GET must distinguish immutable requirements from observed readiness and bind actual Save inputs. |
| Policy marker | Agree, strengthen enforcement: hand-bumped marker plus Needs re-review and admission block. | Cannot freeze arbitrary code semantics; marker discipline and deployment tests are required. |
| Spend | Agree: locked bounded-turn multiplication, no reservation ledger. Existing report wallet lock at `afd_fleet/src/lease/sql/report.rs:130–135`. | Runtime debits can reduce funds after unlock; uncertain attempts stay counted. No zero-overdraft claim. |
| Schedule counter | Agree: new content counter, not claim generation; `afd_cron/src/sql.rs:123,148`. | L8 is a follow-up; stable-ID fix does not fence a late provider write. |
| Steer/status read | Agree with correction: event detail cannot read an unleased steer; `afd_api_tenant/src/handler/event/mod.rs:193–241`. | L1 explicitly amends generic REST guidance. Message recovery must be implemented/proved. |
| Token bounds | Agree: selected tokenizer,32,768 input/call,8,192 output/turn, small edits. | Higher worst-case cost; count tool/system/reasoning tokens and model window. |
| Library | Agree: fleet-libraries only, explicit publication and pinned immutable revisions. | A Save without publication is deliberately a one-off fleet setup. |
| One PR | Owner instruction controls: one current branch/worktree with linked specs. | Larger review surface requires section-level proofs and coordinated release. |

Six earlier corrections remain accepted: Stop is a sequence cutoff; one replay identity per action; fleet:write does not gain Delete; overall schema work is required; onboarding-refresh is live; read-back cannot fence an in-flight provider write.

Crate paths above are relative to `rustd/crates/`. The [five source findings](source-findings.md) give full paths and reasoning. Reviewer files remain read-only review inputs, not implementation specifications.

### Astra High required-change ledger

Indy supplied the Codex Aiwa Astra High review on 2026-10-14: “APPROVE WITH REQUIRED CHANGES.” It reviewed working-tree documents/source at the recorded baseline. That verdict does not approve implementation, passing tests, or deployment. All ten findings are accepted as document/spec repairs below; proofs remain pending.

| Finding | Source assessment and revised requirement | Required implementation proof |
|---|---|---|
| 1 / P1 scoped model access | Raw-key runtime and absent proxy are confirmed in the source ledger. [Canonical invocation](../../architecture/api_resources.md) now specifies opaque issuance, runner/model/attempt bindings, daemon transport, expiry/revocation, and durable single-dispatch receipts. This adds a fourth machine route, leaving 17/7/2 person routes intact. | M227_001 3.2 is now integration: real authorization rejects foreign/expired/revoked capabilities, replayed physical dispatch, model overrides, and runtime credential minting. |
| 2 / P1 capacity renewal | The original-expiry guard is absent. [Runner capacity](../../architecture/runner_fleet.md#authoring-leases) requires both renewal kinds and claims to serialize on the runner row with fresh database time. Expiry cannot revive. | 3.1 barriers two replicas at expiry, replacement admission, and delayed runtime renewal; assert one live slot and no deadlock. |
| 3 / P1 candidate identity | The old lease/ack could not identify the next edit. [Runner wire](../../architecture/api_resources.md) includes candidate and returns accepted ID/revision. Every definition revision gets a fresh immutable ID; exact receipt replay keeps its original version. | 3.2 initialize → edit → edit, lost acknowledgement, stale/reordered proposals, and small edits to 32 KiB instructions without complete retransmission. |
| 4 / P1 receipt-less admission | Existing message reads exclude null receipts despite successful append deferral. [C4 recovery](../../architecture/api_resources.md) requires durable pending admissions in the existing queued shape and logical-ID deduplication. Runtime baseline now distinguishes ordinary acceptance from stronger start.accepted. | 7.1 and 6.2 fail before append and after append/before receipt, reload/retry/sweep, one visible message and one eventual event. |
| 5 / P2 model replacement | [N5](../../architecture/api_resources.md) accepts authoring_model_ref on idle If-Match PATCH. Send captures the reference; admitted turns keep their pins. Reload retains replacement. | 2.2 and 6.2 disabled-model switch/reload, unauthorized selection, queued/running switch rejection. |
| 6 / P2 durable failure actions | [Message](../../architecture/api_resources.md) gains bounded failure reason/code/action. Reports use a validated reason enum; the daemon selects safe actions and overrides retry when usage is unresolved. | 2.2 and 6.2 accepted → failed → reload for each class; correct owner action with no private/provider diagnostics or credentials. |
| 7 / P2 validator example | [Definition](../../architecture/fleet_definitions.md#validated-definition) now uses checkout-watch and version 1.0.0. Required/optional metadata and separate display title are explicit; existing slug/version validation stands. | 1.1 every supported metadata/policy/manifest field round trip, required-field/type/slug rejection. |
| 8 / P2 spool retention | [Copy limits](../../architecture/runner_fleet.md#authoring-leases) distinguish logical expiry, decryptability, and physical retention. Supervisor handling covers encrypted temporary/rejected/quarantined copies and pre-read startup cleanup. No stopped-host deletion promise remains. | 3.4 restart after deadline before decryption/upload, quarantine/temporary cleanup, clock rollback/cleanup failure, accounting-only settlement without text. |
| 9 / P2 website publication | M227_002 §1 explicitly corrects carried behavioral copy under L7, preserving approved visuals. [Website diagram](website-job-entrance.md#what-the-animation-shows) separates fleet creation from explicit Library publication. Source copy remains an implementation task. | M227_002 1.1 browser asserts ordinary Save claims fleet creation only; separate explicit publication is represented. |
| 10 / P2 coordinated acceptance | [Acceptance procedure](public-changes.md) records actual app/daemon/runner/CLI identities, origins, startup, and read-only preflight before fixture mutation. Website proof uses make _dry_website. | 7.2 mismatched/unknown builds fail before Clerk/bootstrap/cleanup writes; record matched release smoke and failed-release recovery. |

The five original agreements remain qualified as above. No reservation ledger, zero-overdraft claim, additional person route, creation worker, or reopened owner choice is introduced. Arithmetic fixtures now cover fractional rates, physical-dispatch rounding, overflow, varied pinned prices, runtime debit interleaving, and unresolved usage. Existing privacy, cutoff, digest, crash, grant, Kill, publication-pin, and runtime-preservation proofs remain required.

Indy's follow-up also requires sound Rust crate/module structure. The spec and inventory now name production modules, error helpers, and behavior-focused test modules. The requested Exonum checkout is absent; local configuration/admission and Arrow module layouts supplied the inspected prior art. No existing crate-wide cleanup or backend refactor was performed by this document update.

### Dark reasoning: hostile conditions

Assume retries are simultaneous, processes die after external writes, members lose access midstream, clocks drift, wallet prices differ, usage is missing, and stale runners report after deletion. Correctness comes from database guards, immutable source pins, typed authority, deletion generations, exact receipt identity, and negative tests. Provider stable IDs still have an unfenced late-write risk until L8. Nothing in this packet asserts stronger production assurance.

### Pessimistic reasoning: expected costs

Explicit setup retry can leave abandoned installing fleets. Model constraints can reject long drafts. Conservative cost bounds can reject work despite enough funds for a typical answer. Unknown provider usage can block later admission. One PR needs broad reviewer attention and aligned daemon/runner/app/CLI release. These costs are visible and accepted in the chosen design; do not hide them with silent fallback or invented success.

### Optimistic reasoning: useful simplicity

Private conversation reads replace generic operation infrastructure. Existing typed policy preserves gates/network/repository meaning. Installing fleets hold setup progress. Explicit Library publication keeps sharing intentional. Small edits avoid output-window failure. One claim gives a clear shared capacity limit. A focused proof suite can establish these boundaries without building parallel services.

### What changed in the documents

Definition, API, creation, runner, flow, runtime map, and layouts now follow the same route set and all 21 decisions. Prompts ask reviewers to attack enforcement rather than reopen choices. The public handoff lists changed user/API behavior for the documentation repository. M227 owns the cutover; M227_002 folds the existing website/branding batch into the same PR.

## Limits

No backend code, commits, pushes, provider calls, full repository tests, or fresh coverage measurement were performed during this rewrite. The spec structural check is separate from implementation proof. Source review does not establish the separately owned schedule fix's merge/test state.

## Related pages

- [Decisions](decisions.md)
- [Source findings](source-findings.md)
- [Review packet](review-packet.md)
- [Public handoff](public-changes.md)
