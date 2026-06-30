# Architecture Scenarios

One golden end-to-end walkthrough that composes the v2 install, trigger, and execute loop for the hero use case. Read it in isolation and you understand how a real user gets to a real outcome.

| File | What it proves |
|---|---|
| [`gh-pr-reviewer.md`](./gh-pr-reviewer.md) | **The golden path.** John Doe installs the `github-pr-reviewer` from a GitHub repo (the bundle storage journey: GitHub tarball → canonical re-pack → R2 + Postgres), wires the webhook, and a Pull Request (PR) gets reviewed. Marks what is built vs the one piece of trigger plumbing still to land (`pull_request` events). |
| [`slack-channel-resident.md`](./slack-channel-resident.md) | **The Rung-0 on-ramp (forward-looking, M106).** A fact told to `@agentsfleet` in one Slack thread is recalled in a different thread of the same channel — because the memory namespace is the per-channel resident fleet, not the thread. Reactive (read-only, mention-only); the acquisition on-ramp to the durable hired teammate. |

> **Why one scenario.** Earlier the set carried three platform-ops walkthroughs (cold install, self-managed posture, credit-gate drain). Those narratives were consolidated into the topic docs they proved — the canonical facts now live in [`../billing_and_provider_keys.md`](../billing_and_provider_keys.md) (posture + billing + gate) and [`../data_flow.md`](../data_flow.md) (the install/trigger/execute/bill loop). The scenario set keeps a single golden narrative; the invariants below point at those topic docs.
>
> Shipped specs under `docs/v2/done/` may still cite the retired `01_default_install.md` / `02_self_managed.md` / `03_balance_gate.md` by name — those are historical records of what each milestone touched at ship time and are intentionally left intact.

## Cross-cutting decisions these docs encode

These are the load-bearing invariants. Every spec under `docs/v2/` should be readable against them; the canonical home for each is the linked topic doc.

1. **Model-caps endpoint** — `GET https://api.agentsfleet.net/_um/da5b6b3810543fe108d816ee972e4ff8/cap.json` is the single source of truth for model → context cap and per-model token rates. Resolved at API-server boot or at `tenant provider add` time, never at trigger time. See [`../billing_and_provider_keys.md`](../billing_and_provider_keys.md) §9–10.
2. **Overlay at lease time** — when frontmatter carries `model: ""` / `context_cap_tokens: 0` / omits the keys, the control plane overlays from `tenant_providers`, per-field. See [`../billing_and_provider_keys.md`](../billing_and_provider_keys.md).
3. **One credit pool, posture-dependent drain** — `core.tenant_billing.balance_nanos` is a single column; receive + run debits fire under both postures; only the per-run rate differs. No plan tiers. See [`../billing_and_provider_keys.md`](../billing_and_provider_keys.md) §1.
4. **api_key visibility boundary** — platform or self-managed, the api_key exists only in vault, server-side memory, and outbound request headers; never in any user-facing surface. See [`../billing_and_provider_keys.md`](../billing_and_provider_keys.md) §8.
5. **One reasoning loop** — install-time steer, production webhook, cron fire, manual steer, and continuation all enter the lease path with the same envelope and the same SKILL.md prose-driven dispatch. The runtime never branches on actor type. See [`../data_flow.md`](../data_flow.md).
