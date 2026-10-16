---
type: explanation
audience: contributor
verified: 2026-10-14
product_version: 0.60.0
executable: false
---

# Workspace chat: five source findings

## What it is

Answers to the five open questions in Claude's October 14 handoff, checked before the rewrite. Evidence is source inspection at `ad3ed8f92c739b59df71863dbeec81d621dd7d0a`, not a live run or passing test. The [21 owner decisions](decisions.md) stay fixed.

## Why it exists

Existing event history, wallet debits, and install handlers do not prove durable authoring recovery. Each agreement below names what must change.

## How it behaves

### 1. Distinct schedule content counter: agree

`rustd/crates/afd_cron/src/sql.rs:123` and `:148` increment `generation` on claims, including sync and retry. These can leave content unchanged. The L8 follow-up needs a separate counter for cron, timezone, message, or desired-status edits; retries retain the content revision. `rustd/crates/afd_cron/src/verifier.rs:125` checks `validation.sub`: the verifier must bind the exact revision-bearing destination. This does not claim the separate stable-ID fix has landed.

### 2. No creation worker: agree, with a narrower reason

`rustd/crates/afd_fleet_lifecycle/src/install.rs:223` generates a new identity; `:232–250` rolls back a failed finish; `:263–271` activates without checking affected rows. `install/grants.rs:39–69` treats grant seeding failures as best effort. Required changes are a retained identity, grant barrier, and checked activation.

Bounded in-request steps return `201 installing` when approval is missing. Same-key POST or `PATCH {status:"active"}` resumes the retained fleet. A worker cannot approve a human grant. The repository does have runtime sweeps; the narrower agreement is no background fleet-creation worker. Authoring lease recovery is a separate requirement.

### 3. Steer does not prove a status read: agree

`rustd/crates/afd_api_tenant/src/handler/event/mod.rs:193–241` reads stored event history; an unleased steer is not available there. L1 makes accepted conversation messages the durable recovery surface.

The Astra follow-up also checked existing message reads. `rustd/crates/afd_events/src/history/statement/queued.rs:10–14` requires a non-null receipt. `rustd/crates/afd_admission/src/admit_receipt.rs:43–48` can return success while append is deferred. Repair that internal read to show every durable pending admission, preserving its queued shape and logical identity. Prove recovery before append and after append/before receipt; event detail supplies neither proof.

`.orly/docs/REST_API_DESIGN_GUIDELINES.md:145` requires a header key and `:157–169` requires operation resources for asynchronous actions. The [approved amendment](public-changes.md) covers draft and fleet conversation body keys, message reads, and streams. Apply it in the owning harness source before generating the guide or implementing routes. Do not hand-edit managed rules as an incidental documentation change.

### 4. GET draft supplies review: agree, with enforcement on policy drift

`rustd/crates/afd_fleet_runtime/src/config/raw/document.rs:42–98` already types runtime policy. With aliases deferred, Save inputs live on the draft's candidate and selections. GET draft supplies effective setup, immutable requirements, live readiness, and review digest.

`rustd/crates/afd_http/src/auth/ownership.rs:63–74` resolves the workspace owner; `rustd/crates/afd_api_tenant/src/handler/tenant/model_entry.rs:201` selects the caller's account. A joined member needs workspace-owner model and wallet resolution. A hand-bumped `policy_semantics` marker triggers Needs re-review. Admission must block a mismatched marker until a reviewed revision is applied; a badge alone cannot prevent access widening.

### 5. Wallet-locked admission: agree, with a stated bound

`rustd/crates/afd_fleet/src/lease/admit/gates.rs:42–55` reads an estimate and permits a missing wallet. `rustd/crates/afd_billing/src/sql.rs:40` reads balance. Neither proves the new authoring rule. Runtime debits lock the wallet at `lease/sql/report.rs:130–135` and `lease/sql/renew.rs:79–84`, updating it at `report.rs:166` and `renew.rs:117`.

Authoring claim must lock that same owner's wallet while counting outstanding bounded attempts and recording the new one. Missing or unreadable wallets refuse new authoring. For varying models, worst-case cost is the maximum bound among active and proposed attempts, so a cheap new model cannot undercount expensive active work. Count full input, output, and runtime fees with checked integer arithmetic and upward rounding.

This is admission, not reserved funds. Runtime debits after unlock can reduce balance. Unfenced attempts remain counted; unknown provider usage remains unknown. Do not advertise zero overdraft or issue a guessed refund.

### Astra follow-up: additional source boundaries

| Boundary | Inspected evidence | Required change |
|---|---|---|
| Model invocation | `rustd/crates/afd_fleet/src/lease/answer.rs:76–86`; `rustd/crates/afr_providers/src/connect.rs:90–94` | Scoped delivery is absent. Add the daemon capability/invocation path and real authorization tests. |
| Capacity revival | `rustd/crates/afd_fleet/src/lease/sql/renew.rs:78–102` | Renewal lacks original-expiry rejection. Serialize renewal and claim decisions under the runner guard with authoritative time. |
| Metadata | `rustd/crates/afd_fleet_runtime/src/frontmatter/skill.rs:136–155` | Required version and lowercase slug must appear in the canonical example and round trips. |
| Spool copies | `rustd/crates/afr_supervisor/src/report_spool.rs:112–141,155–176` | Temporary/rejected files need authoring expiry handling; stopped hosts cannot promise physical deletion on a timer. |
| Website publication | `ui/packages/website/src/lib/chat-story.ts:25–31` | Carried Save copy claims Library publication. M227_002 must correct it while preserving approved visuals. |
| Acceptance startup | `make/acceptance.mk:13–16`; `ui/packages/app/playwright.acceptance.config.ts:178–186` | The lane launches tests/app, not matching daemon/runner builds. Bind preflight and evidence to all four build identities before fixture mutation. |

[The resolution ledger](codex-response.md) records all ten supplied findings. Source inspection confirms the cited gaps; proposed repairs and tests remain unimplemented.

## Limits

No builds, application tests, live exploit, provider fire, or fresh coverage measurement was performed. Planned new resources have no fabricated source files. Later implementation gates carry runtime evidence.

## Related pages

- [Creation](../../architecture/data_flow.md#fleet-creation-and-setup-recovery)
- [Runner admission](../../architecture/runner_fleet.md#authoring-leases)
- [API and gaps](api.md)
