# Architecture scenarios

Each scenario follows one user outcome from its trigger to its result. Each page separates working code from missing proof.

| File | What it proves |
|---|---|
| [`slack-incident-responder.md`](./slack-incident-responder.md) | A mention in a failed Continuous Integration (CI) run's Slack thread reaches exactly one fleet, which answers in that thread with GitHub and Grafana evidence; asked to, the channel's repairer opens one draft fix Pull Request (PR). Zero, one, and several fleets per channel. Built in M206; the live drill is pending. |
| [`durable-repair.md`](./durable-repair.md) | One three-PR repair scenario: ordinary chat and steering, one active lease, exact-head CI, worker replacement, and controlled Grafana/Slack outcomes. Source exists; combined live acceptance remains unfinished. |
| [`chief-of-staff.md`](./chief-of-staff.md) | Proposed: a person reaches a fleet by its name in the chat, the command line and Slack, and a fleet steers another fleet by id, hears back, and reads other fleets' status and answers with no API key. M252_001 owns the proofs. |

The durable-repair scenario extends the current manual Slack/draft path; it does not mark that extension shipped.
Production launch follows written M23x specifications and demonstrated development autonomy.
M224's manual-first evidence campaign excludes development rollout identity checks, full Linux acceptance and the Elasticsearch-dependent incident demonstration by the owner's 2026-11-01 scope amendment.

> Earlier platform-operations walkthroughs repeated facts from topic pages. The current scenarios link to those pages instead of copying their details.
>
> Shipped specs under `docs/v2/done/` may still cite the retired `01_default_install.md`, `02_self_managed.md` and `03_balance_gate.md` by name. Those specs are history and stay as written.

## Invariants every scenario is readable against

Five decisions shape every walkthrough. Each is stated in full on the page
that owns it; these are pointers, not second copies.

| Invariant | Canonical home |
|---|---|
| The model library is the one source for a model's context cap and token rates | [`../billing_and_provider_keys.md`](../billing_and_provider_keys.md) §"10. The model library: catalogue table + authenticated read" |
| A fleet runs the owner model reference pinned with its setup revision, resolved at lease time | [`../user_flow.md`](../user_flow.md) §"§8.7 Model and context-cap origin (platform vs. self-managed)" |
| One credit pool drains under both postures; only the per-run rate differs | [`../billing_and_provider_keys.md`](../billing_and_provider_keys.md) §"1. The two postures" |
| The provider `api_key` never reaches a user-facing surface | [`../billing_and_provider_keys.md`](../billing_and_provider_keys.md) §"8. The self-managed credential and the api_key visibility boundary" |
| Every trigger enters one lease path; the runtime never branches on actor | [`../data_flow.md`](../data_flow.md) §"B. TRIGGER" |
