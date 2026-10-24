---
type: explanation
audience: contributor
verified: 2026-10-25
product_version: 0.65.0
executable: false
---

# SPACE sandbox design handoff

## What it is

The Elephant-Goldfish Model (EGM) brainstorm and its independent concept review are complete.
The [current brief](../../../concepts_brief.md) records the narrowed direction and later owner decisions.
Detailed design drafting, a dedicated implementation spec, their EGM reviews and runtime proof remain outstanding.

SPACE means `Sandboxed Platform for Agentic Code Execution`.
The proposed `agentsfleet` backend adapts useful SPACE source behind the existing Rust runner interfaces.
The [sandbox evaluation](../../architecture/sandbox_evaluation.md#first-firecracker-wrapper) owns the technical proposal and first-build order.

## Why it exists

The owner requested a small Firecracker wrapper with measured startup behavior and control over future changes.
The proposal reuses SPACE's design and selected source rather than operating its full platform.
This handoff keeps that future backend separate from the unfinished work in pull request 784.

## How it behaves

### Delivery order

1. Finish shared compaction in the [existing active spec](../../v2/active/M230_003_P1_API_CLI_UI_LOADING_TERMINAL_LOGIN.md#6--shared-compaction-and-checkpoint-continuation).
2. Complete the unchanged continuity acceptance and forced compaction proofs on the existing bubblewrap backend.
3. Complete the already approved Billing work, then final verification and the recorded implementation-review disposition for pull request 784.
4. Merge pull request 784 through its normal approval process and verify the development deployment.
5. Start the sandbox implementation in its own spec, branch and pull request from the merged code.

This documentation handoff does not mark compaction, Billing or pull request 784 ready.
The [context lifecycle](../../architecture/context_lifecycle.md) owns the 90 percent compaction target and required proof.
Production promotion follows the release-tag workflow; merging to main triggers the development deployment.

### Design and review record

Six fresh reviewers examined an earlier concept and evaluation before the owner narrowed the first build.
Their reports contain 29 Major findings and two Nits, consolidated into 12 Major findings and one Nit.
Those are concept findings, separate from pull request 784's terminal implementation-review record.

[Review evidence](review-evidence.json) retains every original review file, its text and its SHA-256 digest.
SHA-256 means Secure Hash Algorithm with a 256-bit digest.
The archive preserves historical paths and wording; use current repository links in this handoff for new work.

The reviewed brief's digest is `a6047059beea8a0364c8cf1a0928535d0f5fd6bb5f226af6b18c0cabc0d56709`.
The reviewed evaluation's digest is `5b1bbe13a3722889ad6f90dbdb8c5150d0146915ec822c6489cf5fb28ae52b89`.
The archive includes those frozen inputs, the synthesizer's candidate, the consolidated report, the run record and all six native reports.

Later owner decisions permit invited workspace members to approve and continue the same incident's existing draft under fresh checks.
The narrowed scope starts with guest commands, private repository files and declared file recovery.
The current brief and evaluation record those decisions; historical reports retain their original open questions.

### Conversation and sandbox recovery

Compaction manages the model's working input independently of the sandbox engine.
Sandbox recovery manages saved files, Git state and confirmed command outcomes.
The later task-recovery design must select compatible conversation coverage and saved files without treating summary text as authority.

Private fleet authoring already runs without a runtime sandbox or repository.
A runtime fleet without bound repositories must not require GitHub access or create a checkout.
See the [authoring runner](../../../rustd/crates/afr_supervisor/src/authoring/mod.rs) and [checkout's empty-repository path](../../../rustd/crates/afr_supervisor/src/lease_loop/checkout.rs).

## Limits

The detailed design and dedicated sandbox spec have not been produced or reviewed yet.
Some review citations describe the unmerged working tree; recheck them after pull request 784 merges.
No startup measurement or native Firecracker implementation is established by this review.

Local acceptance starts the runner in an OrbStack Linux machine and uses Compose for its HTTPS proxy.
The read-only probe of `afr-approved-chat` returned `Linux`, `aarch64` and `KVM: device absent`.
KVM means Kernel-based Virtual Machine; Firecracker needs access to `/dev/kvm`.

A container on a qualified Linux host can package the runner, but cannot supply missing hardware virtualization.
The guest also needs a compatible kernel, startup image and proved command transport.
See [local acceptance](../../development.md#local-acceptance) and [Firecracker prerequisites](https://github.com/firecracker-microvm/firecracker/blob/main/docs/getting-started.md#prerequisites).

## Related pages

- [Sandbox agent restart prompt](sandbox-handoff-prompt.md)
- [Orly, EGM and Jev improvement prompt](orly-egm-jev-handoff-prompt.md)
- [Sandbox evidence and proposed scenarios](../../architecture/sandbox_evaluation.md)
- [Existing runner and toolbox](../../architecture/runner_execution.md)
