# Roadmap — deferred and forward-looking direction

> Items intentionally out of v2.0 scope, captured so specs don't foreclose them. Current canon for what ships is [`high_level.md`](./high_level.md) + [`direction.md`](./direction.md) + `docs/v2/{pending,active,done}/`. This file is direction, not a commitment.

## v2.1 — authorization

### Scope-based authorization — ✅ DELIVERED (M104_001)

Authorization is now **scope-based**. The role ladder (`AuthRole = user < operator < admin`) and the `platform_admin` bool were deleted; every capability is an explicit `resource:action` scope on the verified token's `scopes` claim, surfaced as `principal.scopes` and enforced by a single `requireScope` gate against a declarative route→scope table (`src/agentsfleetd/http/route_scopes.zig`). The resource/ownership axis (`authorizeWorkspace`) is unchanged, plus an audited `workspace:any` cross-tenant override. Runner enrollment is gated by the discrete `runner:enroll` scope (independently grantable from `runner:{read,write}`), replacing the old `platform_admin` claim. See [`../AUTH.md`](../AUTH.md) → *Scope catalogue* for the full vocabulary, hierarchy, and provisioning bundles.

### Fleet keys → first-class principal

Today fleet keys (`agt_a`) authenticate via a bespoke handler-local lookup (`integration_grants/handler.zig::authenticateFleet`), not the shared middleware, and never become an `AuthPrincipal` (there is no `AuthMode.fleet_key`). v2.1 revamps them into a first-class principal — a dedicated middleware branch + `AuthMode.fleet_key` + a `fleet_id`-scoped principal — aligning with the reference auth design at `~/Projects/oss/auth.md`. The revamp must also fold in the `Session {uuid}` fleet-identity path that the same handler accepts today.

## v2.1+ — other deferred items

- **Flow-1 active-MITM closure** — URL-fragment public-key binding + HKDF transcript binding. See [`../AUTH.md`](../AUTH.md) *threats this flow does NOT close*.
- **Dashboard token model** — the Backend-For-Frontend (BFF) direction. Deferred; detail currently lives in `AUTH.md` and should move here or into its own spec when revisited.
- **Open fleet (mode C)** — self-enrolling runners. See [`runner_fleet.md`](./runner_fleet.md).
- **Label-scoped sticky affinity — ✅ DELIVERED (M85_001).** The first eligibility gate shipped: `core.fleets.required_tags <@ runner.labels` filters the candidate set before `fleet.runner_affinity.last_runner_id` is applied as a sticky preference. A sticky runner that no longer satisfies a fleet's tags cannot win; the eligible runner wins instead. See `src/agentsfleetd/fleet/assign.zig` and the `sticky hint never overrides eligibility` / `unsatisfiable tags hold then schedule` tests in `placement_eligibility_test.zig`.
- **Trust-scoped sticky affinity** — still deferred. Once runners can be local / low-trust (laptops, untrusted hosts), affinity selection must add **trust class + scope** (allowed tenants/workspaces) and sandbox-tier eligibility before the sticky preference — "prefer the last runner *among the eligible set*," never an override of eligibility. M85_001 intentionally shipped labels only; the trust/scope/tier funnel remains its own security workstream.

## GitHub App event routing — active in M102_005

M102_005 completes the inbound half of the existing GitHub App connector: the callback persists both the encrypted installation handle and the non-secret installation-to-workspace route; `/v1/ingress/github` verifies the platform App signature and selects fleets by explicit repository, event, and approved-grant membership. The old fleet-addressed GitHub webhook remains a supported custom path. Slack keeps its specialized events ingress; Jira and Linear remain credential connectors without inbound event routing in this workstream.

The `github-pr-reviewer` walkthrough remains a target, not a shipped proof, until its repository-bound Pull Request integration test passes. The workstream must update that status from test evidence, never from the presence of handler code alone.

## Runner resilience — shipped in M90_001 (deadlines) + M108_001 (shared watchdog)

- **Control-plane call deadlines — ✅ DELIVERED.** The runner still uses `std.http.Client.fetch`, but every `/v1/runners/me/*` verb now takes a required `deadline_ms`; `control_plane_client.zig` arms a per-client `CallWatchdog` around the pooled socket and shuts the in-flight call down at the bound, so a hung control plane returns a retryable transport failure instead of wedging the worker. Deadlines are env-overridable via `RUNNER_CP_*_DEADLINE_MS`; renew is clamped under the renewal tick/window relation so a stuck renew cannot starve the child deadline kill. M108_001 promoted the watchdog into `src/lib/call_deadline/` for reuse. Residual window: name resolution and initial TCP connect inside `fetch` are still outside the watchdog.

## Fleet operator plane + proactive reassignment — shipped in M84_001 (read) + M84_002 (mutation/reassignment)

M80_006 shipped per-lease renewal (§3 — a *live* runner keeps its lease). The operator plane (§1: `GET`/`PATCH /v1/fleets/runners`, cordon/revoke) and heartbeat-lapse reassignment (§2: expire a *dead* runner's affinity so its work re-leases to a healthy host) were carved out after a design study. Both shipped: the **read** — `GET /v1/fleets/runners` (paginated, platform-admin-gated, **derived** liveness, no `token_hash`) — landed in **M84_001**; the **mutation + reassignment** — `PATCH /v1/fleets/runners/{id}` cordon/drain/revoke, the `status`→`admin_state` typed enum, `UZ-RUN-009`, the append-only `fleet.runner_events` log, and the liveness sweeper that closes §2 — shipped in **M84_002**. The model: typed `admin_state` (intent) + **derived** liveness (runtime, never stored) + `runner_events` (history); **no JSONB status** (cross-validated). The deeper points the study surfaced still hold:

- **All-runners-down.** If every healthy runner is gone, where does cordoned/lapsed work drain to? There is no eligible target — the work must **hold** (not thrash or fail) until capacity returns.
- **Eligibility — which runner can take it?** A cordoned/lapsed runner's work can't route anywhere: the target must satisfy every shipped eligibility gate before sticky routing. Today that means the **M85_001 label gate** (`required_tags ⊆ labels`) plus admin-state/liveness checks; M84_002 reassignment composes with that filter. Trust class, tenant/workspace scope, sandbox-tier requirements, and capacity-aware placement remain future work: the runner has a local `worker_count`, but the control plane does not receive it yet, so `available = worker_count - active` is not enforceable server-side.
- **Cordon rules.** When to cordon; partial vs full drain; the drain deadline; what happens if drain never completes (escalate cordon → revoke?).
- **Drain rules.** How long to wait for in-flight work before reclaiming; how the heartbeat `drain` reply composes with renewal.

Both shipped: the `GET /v1/fleets/runners` read + honest derived liveness landed in **M84_001**, and the `PATCH` mutation surface, `admin_state` (`cordoned`/`revoked`/`draining`/`drained`), `UZ-RUN-009`, event history, and sweeper shipped in **M84_002**. Heartbeat-lapse recovery is still bounded by the lease-expiry backstop + the pull-triggered reclaim that M80_002 already ships; the sweeper adds the audit event and the admin-driven reassignment path.

## Security Reviewer — prebuilt fleet fleet (forward-looking)

A customer-facing prebuilt fleet whose job is **security testing on the customer's own code and infrastructure** (authorized, defensive — not red-teaming the fleet runtime itself, which is the platform's internal sandbox concern). It fits the existing evidence-plus-approval loop: wakes on a pull request or a schedule, scans the diff and dependencies for vulnerabilities and exposed secrets, reproduces the finding as a scenario, opens a remediation pull request with the evidence attached, and **holds the fix at human approval** while flagging the team in Slack. Integrations: GitHub (code / pull requests) + Slack (alerts); no new credential class beyond what the review and incident fleets already use. Captured here because it surfaced as product direction (marketing showcase + customer ask) before any spec — so spec authors don't foreclose it. Not part of v2.0 scope.

## Slack-resident surface — the consumption ladder (M106 + follow-on)

Where the human front door points after the CLI/dashboard wedge. Rung 0 shipped as `docs/v2/done/M106_001_P1_API_DOCS_INFRA_UI_SLACK_RESIDENT_CHANNEL_BOT.md`; the hired-teammate follow-on is not yet specced. **The follow-on is direction, not a commitment.**

The product is reached through a two-rung ladder whose boundary is **agency, not memory**:

- **Rung 0 — channel-resident reactive bot — ✅ DELIVERED (M106_001).** A first-party multi-tenant `@agentsfleet` Slack app: one OAuth (Open Authorization) install per Slack workspace (`team_id → workspace`). In any channel it's invited to, an `@mention` is answered in-thread, read-only, mention-only — and the bot **learns that channel** over time. The memory namespace is a **per-channel resident fleet** (memory is keyed by `fleet_id`, not workspace), so memory persists across threads because the fleet, not the thread, owns it (reuses the [`runner_fleet.md`](./runner_fleet.md) §Memory-continuity loop verbatim). It never acts unattended.
- **Rung 1 — hired durable teammates (follow-on).** From the same Slack surface, a recurring need converts into a durable teammate that subscribes to a real source (e.g. Zoho Desk), wakes unattended, and takes **gated** write actions with approval — the existing event-driven runtime. The Slack surface adds library-install + per-integration OAuth connectors + the Slack-user → `approval:resolve` allowlist. Depends on M103 (Fleet library) + M105 (schedules).

**Why this is not "a chat UI over tools"** ([`high_level.md`](./high_level.md) §1): Rung 0 is the acquisition on-ramp, deliberately reactive — its job is to be useful enough to convert to the durable teammate. The durable runtime is still the product; agency (acting unattended) is what the operator hires and what a reactive channel bot structurally cannot do. Memory is free at both rungs.

## Bastion — post-MVP shape

Where the v2 wedge points after launch. Not part of v2; documented so spec authors don't foreclose it.

The MVP ships an internal-only diagnosis posted to the operator's Slack. The longer-term play is the **bastion** — one durable surface where internal triage continues as today (Slack post, evidence trail, follow-up steers) and external customer communication is derived from the *same* incident state (status-page updates, broadcast email/SMS, embedded widgets). The same fleet owns both; the diagnosis and the customer-facing narrative come from one event log, not two. This is the structural competitor to manual status-page tools.

Structural changes from MVP to bastion:

1. **Per-fleet audience routing** — `TRIGGER.md` / `x-agentsfleet:` gains `audiences: [internal_slack, customer_status, customer_email]`; `SKILL.md` prose drafts per-audience summaries from the same evidence.
2. **Status-page rendering surface** — a hosted page at `status.<customer-domain>` renders the latest `processed` event's customer-facing summary.
3. **Broadcast channels** — the fleet's `tools:` grows `email_send`, `sms_send` (approval-gated for a first incident), `webhook_post` (Statuspage / PagerDuty downstream).
4. **Approval gating per audience** — `SKILL.md` can require human approval for customer-facing audiences while internal Slack flows automatically (the M47 approval inbox handles the mechanic).
5. **Per-actor retention** — customer-facing communications carry stricter retention (Sarbanes-Oxley Act (SOX), General Data Protection Regulation (GDPR)); `core.fleet_events` retention becomes per-actor configurable.

What does not change: the runtime architecture, the sandbox boundary, the trigger model, and the secret vault / network policy / budget caps / context lifecycle. Bastion audience routing applies to work-events only — worker-emitted `system:*` rows stay on the internal operator timeline. The bastion is a `SKILL.md` authoring pattern plus a few tool primitives plus a rendering surface — not a different product.
