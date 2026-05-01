# Architecture Scenarios

Three end-to-end walkthroughs that compose the v2 install, trigger, execute, and bill loop. Each is a complete narrative — you should be able to read one in isolation and understand how a real user gets to a real outcome.

All three scenarios follow the same persona — **John Doe** — across his journey from cold install to BYOK adoption to gate trip.

| File | Phase of John's journey | What it proves |
|---|---|---|
| [`01_default_install.md`](./01_default_install.md) | Cold install on platform-managed (Anthropic + Sonnet) | Wedge demo: zero to first webhook diagnosis in <10 min. Doctor returns the synth-default `tenant_provider` block; install-skill writes resolved values into frontmatter. |
| [`02_byok.md`](./02_byok.md) | Switches to BYOK (Fireworks + Kimi 2.6) | Tenant-scoped provider flip; cap resolves into `tenant_providers` at `tenant provider set` time; worker overlays sentinels at trigger time; api_key never leaves the resolver-to-executor path. |
| [`03_balance_gate.md`](./03_balance_gate.md) | $10 credit grant drains across both postures, then exhausts | Credit pool drains under both postures; same gate code path; posture-dependent receive + stage deductions; gate trips at zero with a dashboard-pointer UX. |

## Cross-cutting decisions these scenarios encode

1. **Model-caps endpoint** — `GET https://api.usezombie.com/_um/da5b6b3810543fe108d816ee972e4ff8/model-caps.json` (cryptic-prefix to dodge opportunistic crawlers) is the single source of truth for model → context cap **and per-model token rates**. Resolved at API-server boot for the rate cache, or at `tenant provider set` time for cap. Never resolved at trigger time. See [`02_byok.md`](./02_byok.md) §5 and [`../billing_and_byok.md`](../billing_and_byok.md) §10 for the endpoint shape.
2. **Worker overlay** — when frontmatter carries `model: ""` or `context_cap_tokens: 0` or omits the keys entirely, the worker overlays from `tenant_providers`. Per-field, independent. Visible sentinels for human readability; absent-key as the safety net for hand-edits.
3. **One credit pool, posture-dependent drain** — `core.tenant_billing.balance_cents` is a single column. Receive + stage debits both fire under both postures; only the cents differ. No plan tiers in the cost function; no included-events ladder.
4. **api_key visibility boundary** — platform OR BYOK, the api_key exists only in vault, server-side process memory, and outbound HTTPS request headers. It never appears in any user-facing surface (doctor JSON, HTTP responses, logs, agent context, persisted event rows). See [`../billing_and_byok.md`](../billing_and_byok.md) §8.
5. **One reasoning loop** — install-time steer, production webhook, cron fire, manual steer, and continuation event all enter `processEvent` with the same envelope shape and the same SKILL.md prose-driven dispatch. The runtime never branches on actor type.

These four are the load-bearing invariants. Every spec under `docs/v2/` should be readable against them.
