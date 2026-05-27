# Roadmap — deferred and forward-looking direction

> Items intentionally out of v2.0 scope, captured so specs don't foreclose them. Current canon for what ships is [`high_level.md`](./high_level.md) + [`direction.md`](./direction.md) + `docs/v2/{pending,active,done}/`. This file is direction, not a commitment.

## v2.1 — authorization

### Scope-based authorization (designed at the API level now, enforced in v2.1)

Today authorization is **role-based**: `AuthRole = user < operator < admin` (`src/auth/rbac.zig`), enforced by the `RequireRole` middleware. JWTs already carry a `scope`/`scopes` claim — `src/auth/claims.zig` parses it — but the middleware frees it and never puts it on `AuthPrincipal`. So scopes are parsed-but-discarded; nothing is enforced on them.

v2.1 wires scope enforcement at the API level so a capability can be granted without handing out a whole role. The API surface is shaped for it now:

- **`fleet:write`** — register / cordon runners. Runner provisioning carries cross-tenant blast radius (a trusted-fleet runner receives any tenant's secrets inline), so a dedicated scope is tighter than the blunt `admin` role.
- finer tenant scopes (`runs:read`, `runs:write`, `workspace:pause`, …) on api keys / JWTs, replacing the all-or-nothing `zmb_t_ = admin` grant.

Until v2.1, runner registration is gated by `RequireRole{.admin}` (see [`../AUTH.md`](../AUTH.md) → *Runner token*). The scope names above are the v2.1 target, documented so the `/v1/runners` surface is designed for them.

### Agent keys → first-class principal

Today agent keys (`zmb_`) authenticate via a bespoke handler-local lookup (`integration_grants/handler.zig::authenticateZombie`), not the shared middleware, and never become an `AuthPrincipal` (there is no `AuthMode.agent_key`). v2.1 revamps them into a first-class principal — a dedicated middleware branch + `AuthMode.agent_key` + a `zombie_id`-scoped principal — aligning with the reference auth design at `~/Projects/oss/auth.md`. The revamp must also fold in the `Session {uuid}` zombie-identity path that the same handler accepts today.

## v2.1+ — other deferred items

- **Flow-1 active-MITM closure** — URL-fragment public-key binding + HKDF transcript binding. See [`../AUTH.md`](../AUTH.md) *threats this flow does NOT close*.
- **Dashboard token model** — the Backend-For-Frontend (BFF) direction. Deferred; detail currently lives in `AUTH.md` and should move here or into its own spec when revisited.
- **Open fleet (mode C)** — self-enrolling runners. See [`runner_fleet.md`](./runner_fleet.md).

## Bastion — post-MVP shape

Where the v2 wedge points after launch. Not part of v2; documented so spec authors don't foreclose it.

The MVP ships an internal-only diagnosis posted to the operator's Slack. The longer-term play is the **bastion** — one durable surface where internal triage continues as today (Slack post, evidence trail, follow-up steers) and external customer communication is derived from the *same* incident state (status-page updates, broadcast email/SMS, embedded widgets). The same zombie owns both; the diagnosis and the customer-facing narrative come from one event log, not two. This is the structural competitor to manual status-page tools.

Structural changes from MVP to bastion:

1. **Per-zombie audience routing** — `TRIGGER.md` / `x-usezombie:` gains `audiences: [internal_slack, customer_status, customer_email]`; `SKILL.md` prose drafts per-audience summaries from the same evidence.
2. **Status-page rendering surface** — a hosted page at `status.<customer-domain>` renders the latest `processed` event's customer-facing summary.
3. **Broadcast channels** — the zombie's `tools:` grows `email_send`, `sms_send` (approval-gated for a first incident), `webhook_post` (Statuspage / PagerDuty downstream).
4. **Approval gating per audience** — `SKILL.md` can require human approval for customer-facing audiences while internal Slack flows automatically (the M47 approval inbox handles the mechanic).
5. **Per-actor retention** — customer-facing communications carry stricter retention (Sarbanes-Oxley Act (SOX), General Data Protection Regulation (GDPR)); `core.zombie_events` retention becomes per-actor configurable.

What does not change: the runtime architecture, the sandbox boundary, the trigger model, and the credential vault / network policy / budget caps / context lifecycle. Bastion-stage audience routing applies to work-events only — worker-emitted `system:*` rows stay on the internal operator timeline. The bastion is a `SKILL.md` authoring pattern plus a few tool primitives plus a rendering surface — not a different product.
