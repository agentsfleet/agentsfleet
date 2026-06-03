# Roadmap — deferred and forward-looking direction

> Items intentionally out of v2.0 scope, captured so specs don't foreclose them. Current canon for what ships is [`high_level.md`](./high_level.md) + [`direction.md`](./direction.md) + `docs/v2/{pending,active,done}/`. This file is direction, not a commitment.

## v2.1 — authorization

### Scope-based authorization (designed at the API level now, enforced in v2.1)

Today authorization is **role-based**: `AuthRole = user < operator < admin` (`src/zombied/auth/rbac.zig`), enforced by the `RequireRole` middleware. JWTs already carry a `scope`/`scopes` claim — `src/zombied/auth/claims.zig` parses it — but the middleware frees it and never puts it on `AuthPrincipal`. So scopes are parsed-but-discarded; nothing is enforced on them.

v2.1 wires scope enforcement at the API level so a capability can be granted without handing out a whole role. The API surface is shaped for it now:

- **`fleet:write`** — register / cordon runners. Runner provisioning carries cross-tenant blast radius (a trusted-fleet runner receives any tenant's secrets inline), so a dedicated scope is tighter than the blunt `admin` role.
- finer tenant scopes (`runs:read`, `runs:write`, `workspace:pause`, …) on api keys / JWTs, replacing the all-or-nothing `zmb_t_ = admin` grant.

Until v2.1, runner registration is gated by `RequireRole{.admin}` (see [`../AUTH.md`](../AUTH.md) → *Runner token*). The scope names above are the v2.1 target, documented so the `/v1/runners` surface is designed for them.

### Agent keys → first-class principal

Today agent keys (`zmb_`) authenticate via a bespoke handler-local lookup (`integration_grants/handler.zig::authenticateZombie`), not the shared middleware, and never become an `AuthPrincipal` (there is no `AuthMode.agent_key`). v2.1 revamps them into a first-class principal — a dedicated middleware branch + `AuthMode.agent_key` + a `zombie_id`-scoped principal — aligning with the reference auth design at `~/Projects/oss/auth.md`. The revamp must also fold in the `Session {uuid}` agent-identity path that the same handler accepts today.

## v2.1+ — other deferred items

- **Flow-1 active-MITM closure** — URL-fragment public-key binding + HKDF transcript binding. See [`../AUTH.md`](../AUTH.md) *threats this flow does NOT close*.
- **Dashboard token model** — the Backend-For-Frontend (BFF) direction. Deferred; detail currently lives in `AUTH.md` and should move here or into its own spec when revisited.
- **Open fleet (mode C)** — self-enrolling runners. See [`runner_fleet.md`](./runner_fleet.md).

## Runner resilience — deferred to the Zig 0.16 toolchain bump

- **Control-plane read-timeout** — the runner drives every `/v1/runners/me/*` call through `std.http.Client.fetch` in `src/runner/daemon/control_plane_client.zig` (`post`), which exposes **no timeout knob in Zig 0.15.2**. A hung control plane therefore wedges the runner until the operating-system socket timeout eventually fires. The native fix is `fetch`'s `timeout` field, which first lands in **Zig 0.16** (`std.http.Client` gains `timeout: Io.Timeout`) — wire it at the `post` call site when the toolchain bumps. The only in-0.15.2 alternatives were rejected as heavier than the gap: a detached-thread watchdog with a leaked, self-cleaning result mailbox (use-after-free hazard the moment it shares the per-call request arena), or the manual `open()`/`readVec()` socket path that is Linux-broken under 0.15. Interim guard: the boot-path watchdog in `src/runner/daemon/loop.zig` bounds the test only, not production. Surfaced during M80_005 (PR #351).

## Fleet operator plane + proactive reassignment — needs a design study (deferred from M80_006 §1/§2)

M80_006 shipped per-lease renewal (§3 — a *live* runner keeps its lease). The operator plane (§1: `GET`/`PATCH /v1/fleet/runners`, cordon/revoke) and heartbeat-lapse reassignment (§2: expire a *dead* runner's affinity so its work re-leases to a healthy host) were carved out into their own spec after a design study — the reassignment-target problem is deeper than a `status` flip, and the **cordon/drain/eligibility ruleset is the real deliverable, not the HTTP surface**:

- **All-runners-down.** If every healthy runner is gone, where does cordoned/lapsed work drain to? There is no eligible target — the work must **hold** (not thrash or fail) until capacity returns.
- **Eligibility — which runner can take it?** A cordoned/lapsed runner's work can't route anywhere: the target must satisfy trust class, scope, sandbox tier, and capacity — the same filter the M80_007 scheduler owns. Reassigning without that filter risks routing prod / other-tenant work to an unfit host (see *Local-runner affinity trust scope*, runner_fleet.md).
- **Cordon rules.** When to cordon; partial vs full drain; the drain deadline; what happens if drain never completes (escalate cordon → revoke?).
- **Drain rules.** How long to wait for in-flight work before reclaiming; how the heartbeat `drain` reply composes with renewal.

These need study before code. The `/v1/fleet/runners` surface, `RUNNER_STATUS_{cordoned,revoked}`, and the `UZ-RUN-009` "Runner revoked" code were left **unbuilt** so the design isn't foreclosed (`UZ-RUN-009` stays unwired until the spec lands). Heartbeat-lapse recovery is for now bounded by the lease TTL backstop + the pull-triggered reclaim that M80_002 already ships.

## Bastion — post-MVP shape

Where the v2 wedge points after launch. Not part of v2; documented so spec authors don't foreclose it.

The MVP ships an internal-only diagnosis posted to the operator's Slack. The longer-term play is the **bastion** — one durable surface where internal triage continues as today (Slack post, evidence trail, follow-up steers) and external customer communication is derived from the *same* incident state (status-page updates, broadcast email/SMS, embedded widgets). The same agent owns both; the diagnosis and the customer-facing narrative come from one event log, not two. This is the structural competitor to manual status-page tools.

Structural changes from MVP to bastion:

1. **Per-agent audience routing** — `TRIGGER.md` / `x-usezombie:` gains `audiences: [internal_slack, customer_status, customer_email]`; `SKILL.md` prose drafts per-audience summaries from the same evidence.
2. **Status-page rendering surface** — a hosted page at `status.<customer-domain>` renders the latest `processed` event's customer-facing summary.
3. **Broadcast channels** — the agent's `tools:` grows `email_send`, `sms_send` (approval-gated for a first incident), `webhook_post` (Statuspage / PagerDuty downstream).
4. **Approval gating per audience** — `SKILL.md` can require human approval for customer-facing audiences while internal Slack flows automatically (the M47 approval inbox handles the mechanic).
5. **Per-actor retention** — customer-facing communications carry stricter retention (Sarbanes-Oxley Act (SOX), General Data Protection Regulation (GDPR)); `core.zombie_events` retention becomes per-actor configurable.

What does not change: the runtime architecture, the sandbox boundary, the trigger model, and the credential vault / network policy / budget caps / context lifecycle. Bastion audience routing applies to work-events only — worker-emitted `system:*` rows stay on the internal operator timeline. The bastion is a `SKILL.md` authoring pattern plus a few tool primitives plus a rendering surface — not a different product.
