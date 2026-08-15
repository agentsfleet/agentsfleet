# Architecture scenarios

Each scenario follows one user outcome from its trigger to its result. Each page separates working code from missing proof.

| File | What it proves |
|---|---|
| [`github-pr-reviewer.md`](./github-pr-reviewer.md) | Install the `github-pr-reviewer` library and receive review comments on a Pull Request (PR). |
| [`production-deploy-repair.md`](./production-deploy-repair.md) | Diagnose a failed production deployment, prepare a bounded fix, and hold the draft PR for human review. |
| [`slack-channel-resident.md`](./slack-channel-resident.md) | **The Rung-0 on-ramp (M106).** A fact told to `@agentsfleet` in one Slack thread is recalled in a different thread of the same channel — because the memory namespace is the per-channel resident fleet, not the thread. Reactive (read-only, mention-only); the acquisition on-ramp to the durable hired teammate. |

> Earlier platform-operations walkthroughs repeated facts from topic pages. The current scenarios link to those pages instead of copying their details.
>
> Shipped specs under `docs/v2/done/` may still cite the retired `01_default_install.md` / `02_self_managed.md` / `03_balance_gate.md` by name — those are historical records of what each milestone touched at ship time and are intentionally left intact.

## Invariants every scenario is readable against

Five decisions shape all three walkthroughs. Each is stated in full on the page
that owns it; these are pointers, not second copies.

| Invariant | Canonical home |
|---|---|
| The model library is the one source for a model's context cap and token rates | [`../billing_and_provider_keys.md`](../billing_and_provider_keys.md) §10 |
| Missing frontmatter model fields overlay from the tenant's selection, per field, at lease time | [`../billing_and_provider_keys.md`](../billing_and_provider_keys.md) |
| One credit pool drains under both postures; only the per-run rate differs | [`../billing_and_provider_keys.md`](../billing_and_provider_keys.md) §1 |
| The provider `api_key` never reaches a user-facing surface | [`../billing_and_provider_keys.md`](../billing_and_provider_keys.md) §8 |
| Every trigger enters one lease path; the runtime never branches on actor | [`../data_flow.md`](../data_flow.md) |>>>>>>> origin/main
