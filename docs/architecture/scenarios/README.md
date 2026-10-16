# Architecture scenarios

Each scenario follows one user outcome from its trigger to its result. Each page separates working code from missing proof.

| File | What it proves |
|---|---|
| [`github-pr-reviewer.md`](./github-pr-reviewer.md) | Save the `github-pr-reviewer` setup and have it read a Pull Request (PR); the review post is refused at egress today. |
| [`production-deploy-repair.md`](./production-deploy-repair.md) | Diagnose a failed production deployment, prepare a bounded fix, and hold the draft PR for human review. |
| [`slack-channel-resident.md`](./slack-channel-resident.md) | **The Rung-0 on-ramp (M106).** A fact told to `@agentsfleet` in one Slack thread is recalled in another thread of the same channel, because memory belongs to the channel's resident fleet. The resident is read-only and answers only mentions. Shipped in `agentsfleetd` (M206_002). |
| [`two-fleets-github-slack.md`](./two-fleets-github-slack.md) | Two fleets in one workspace, one picture per step: a PR wakes one, a Slack mention wakes the other, a steer queues behind the first; the Slack answer returns to its thread and the PR answer stays in the dashboard. |
| [`slack-incident-responder.md`](./slack-incident-responder.md) | A mention in a failed Continuous Integration (CI) run's Slack thread reaches exactly one fleet, which answers in that thread with GitHub and Grafana evidence; asked to, the channel's repairer opens one draft fix Pull Request (PR). Zero, one, and several fleets per channel. Built in M206; the live drill is pending. |

> Earlier platform-operations walkthroughs repeated facts from topic pages. The current scenarios link to those pages instead of copying their details.
>
> Shipped specs under `docs/v2/done/` may still cite the retired `01_default_install.md`, `02_self_managed.md` and `03_balance_gate.md` by name. Those specs are history and stay as written.

## Invariants every scenario is readable against

Five decisions shape every walkthrough. Each is stated in full on the page
that owns it; these are pointers, not second copies.

| Invariant | Canonical home |
|---|---|
| The model library is the one source for a model's context cap and token rates | [`../billing_and_provider_keys.md`](../billing_and_provider_keys.md) §"10. The model library — catalogue table + authenticated read" |
| A fleet runs the owner model reference pinned with its setup revision, resolved at lease time | [`../user_flow.md`](../user_flow.md) §"§8.7 Model and context-cap origin (platform vs. self-managed)" |
| One credit pool drains under both postures; only the per-run rate differs | [`../billing_and_provider_keys.md`](../billing_and_provider_keys.md) §"1. The two postures" |
| The provider `api_key` never reaches a user-facing surface | [`../billing_and_provider_keys.md`](../billing_and_provider_keys.md) §"8. The self-managed credential and the api_key visibility boundary" |
| Every trigger enters one lease path; the runtime never branches on actor | [`../data_flow.md`](../data_flow.md) §"B. TRIGGER" |
