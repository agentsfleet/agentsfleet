# Astra review of the expanded M236 draft

Native reviewer `/root/autonomy_astra_completeness`, gpt-6-astra at max reasoning, reviewed the initial 289-line draft after the scenario reviews.
Reviewed SHA-256: `fc5aedd434edfffab94110cdbb34cfafb14f67ffb10552f38b5a60afe4796d53`.
Verdict: coherent scope suitable for a pending checkpoint PR, with no unsafe invented mechanism; not implementation-ready.

## Findings and disposition

- Major: proof mapping omitted explicit status/refresh, full task-stop states and uncertain notification delivery. Added them to Sections 2/6 and named proofs, including notification receipt reconciliation.
- Major's related proof-design gap: model/manual procedures and evidence destination were vague. Added the case-set categories, pre-trigger freeze, per-case grading and concise native ledger destination; final exact inputs remain a blocker before implementation acceptance.
- Nit: normalized proof tiers, added the required gitleaks row and pinned dry browser tests outside the authenticated acceptance directory.
- Nit: replaced the nonexistent test rule and generic auth reference with installed SPEC-E2E, auth, residual-state and acknowledgement-loss rule identifiers.

The detailed store/wait transitions, publication precondition, actual Jev adapter/accounting, exact interfaces and recovery limits remain explicitly unresolved before code.
This follow-up does not complete the full EGM workflow or prove runtime behavior.

## Read evidence

The reviewer read the full spec standard/template, draft and review index; compared scenario human-input, ownership/events, Grafana, progress/failure and acceptance sections.
Source checks covered configured gate commands, make/dry.mk, the browser configuration, installed rule identifiers and referenced locations.
No edits, tests, builds, Git mutations or external calls occurred in the review.
