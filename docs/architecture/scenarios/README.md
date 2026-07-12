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

## Cross-cutting decisions these docs encode

These are the load-bearing invariants. Every spec under `docs/v2/` should be readable against them; the canonical home for each is the linked topic doc.

1. **Model library** — the `core.model_library` table is the single source of truth for model → context cap and per-model token rates (tenant read: bearer-authed `GET /v1/models`; the former public cap.json route is retired). Resolved at API-server boot or at `tenant provider create` time, never at trigger time. See [`../billing_and_provider_keys.md`](../billing_and_provider_keys.md) §10.
2. **Overlay at lease time** — when frontmatter carries `model: ""` / `context_cap_tokens: 0` / omits the keys, the control plane overlays from `tenant_model_selection`, per-field. See [`../billing_and_provider_keys.md`](../billing_and_provider_keys.md).
3. **One credit pool, posture-dependent drain** — `core.tenant_billing.balance_nanos` is a single column; receive + run debits fire under both postures; only the per-run rate differs. No plan tiers. See [`../billing_and_provider_keys.md`](../billing_and_provider_keys.md) §1.
4. **api_key visibility boundary** — platform or self-managed, the api_key exists only in vault, server-side memory, and outbound request headers; never in any user-facing surface. See [`../billing_and_provider_keys.md`](../billing_and_provider_keys.md) §8.
5. **One reasoning loop** — install-time steer, production webhook, cron fire, manual steer, and continuation all enter the lease path with the same envelope and the same SKILL.md prose-driven dispatch. The runtime never branches on actor type. See [`../data_flow.md`](../data_flow.md).
