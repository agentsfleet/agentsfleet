//! SQL statement text for the runner control-plane handlers (RULE SQLMOD —
//! query text lives here, grepable in one place).
//!
//! Every read here is runner-scoped: a runner principal authorizes only verbs
//! about itself, so each statement carries `runner_id` in its predicate and can
//! never resolve another runner's row.

/// Enrol a runner and record the enrolment event atomically, so a registered
/// runner always has the audit row that explains where it came from. The
/// operator's ASSIGNED policy lands on the row here — the host never writes it.
pub const INSERT_RUNNER_WITH_EVENT =
    \\WITH inserted AS (
    \\  INSERT INTO fleet.runners
    \\  (id, host_id, token_hash, sandbox_tier, admin_state, labels, tenant_id,
    \\   last_seen_at, created_at, updated_at, network_policy, registry_allowlist, worker_count,
    \\   degraded, degraded_reason)
    \\VALUES ($1::uuid, $2::text, $3::text, $4::text, $5::text, $6::jsonb, NULL, $7::bigint, $8::bigint, $8::bigint,
    \\        $13::text, $14::jsonb, $15::int, $16::bool, $17::text)
    \\  RETURNING id
    \\)
    \\INSERT INTO fleet.runner_events
    \\  (id, runner_id, event_type, metadata, dedup_key, created_at)
    \\SELECT $9::uuid, id, $10::text,
    \\       jsonb_build_object($11::text, $2::text, $12::text, $4::text),
    \\       NULL, $8::bigint
    \\FROM inserted
;

/// `GET /v1/runners/me`. Deliberately omits `token_hash` — the self read is
/// used by the operator CLI's `status`, and a credential must never round-trip.
pub const SELECT_RUNNER_SELF =
    \\SELECT id::text, admin_state, host_id, sandbox_tier, last_seen_at,
    \\       network_policy, registry_allowlist::text, worker_count,
    \\       capability_report::text, degraded, degraded_reason, extra_binds::text
    \\FROM fleet.runners WHERE id = $1::uuid
;

/// Heartbeat's policy read — assignment, stored capability, and the prior
/// verdict, so every beat reconciles and carries the current truth back.
/// `selftest_requested_at` rides the same read: an outstanding operator request
/// reaches the host on the next beat without a second query or a second
/// endpoint.
pub const SELECT_RUNNER_ASSIGNED_POLICY =
    \\SELECT sandbox_tier, network_policy, registry_allowlist::text, worker_count,
    \\       degraded, degraded_reason, capability_report::text, selftest_requested_at,
    \\       extra_binds::text
    \\FROM fleet.runners WHERE id = $1::uuid
;

/// Store a reported self-test verdict and retire the request that asked for it,
/// in one write.
///
/// `selftest_requested_at = NULL` is what makes the request one-shot: the beat
/// that reports a verdict is the beat that clears the ask, so a runner cannot
/// re-run the probe every interval until an operator intervenes.
///
/// The clear is unconditional rather than matched against the request that
/// prompted it. A startup probe arrives with no request outstanding and simply
/// writes NULL over NULL; and if an operator re-requests while a verdict is in
/// flight, losing that request costs one click, whereas matching would need a
/// request token on the wire to gain it.
pub const UPDATE_RUNNER_SELFTEST =
    \\UPDATE fleet.runners
    \\SET selftest_checks = $2::jsonb, selftest_all_ok = $3::bool,
    \\    selftest_sandbox_tier = $4::text, selftest_network_policy = $5::text,
    \\    selftest_completed_at = $6::bigint, selftest_requested_at = NULL,
    \\    updated_at = $6::bigint
    \\WHERE id = $1::uuid
;

/// Store a fresh capability report and the reconciled verdict in one write.
pub const UPDATE_RUNNER_CAPABILITY_AND_VERDICT =
    \\UPDATE fleet.runners
    \\SET capability_report = $2::jsonb, capability_reported_at = $3::bigint,
    \\    degraded = $4::bool, degraded_reason = $5::text, updated_at = $3::bigint
    \\WHERE id = $1::uuid
;

/// Re-reconcile against the stored report (no fresh report this beat); the
/// guard makes a steady state write nothing at all.
pub const UPDATE_RUNNER_VERDICT =
    \\UPDATE fleet.runners
    \\SET degraded = $2::bool, degraded_reason = $3::text, updated_at = $4::bigint
    \\WHERE id = $1::uuid
    \\  AND (degraded IS DISTINCT FROM $2::bool OR degraded_reason IS DISTINCT FROM $3::text)
;

/// Resolve a live lease's billing scope before minting a credential for it.
/// The status and expiry predicates are the authorization: an expired or
/// released lease resolves nothing, so no credential can be minted against it.
/// The fleet's `config_json` rides the same row so the repository EGRESS binding
/// costs no second round trip: the mint needs it to scope the token, and this
/// query already resolves the fleet the lease belongs to. Joined rather than
/// read separately so the binding can never be resolved from a different fleet
/// than the one the lease authorized.
pub const SELECT_LEASE_SCOPE_FOR_MINT =
    \\SELECT l.workspace_id::text, l.fleet_id::text, f.config_json::text, l.event_id
    \\FROM fleet.runner_leases l
    \\JOIN core.fleets f ON f.id = l.fleet_id
    \\WHERE l.id = $1::uuid AND l.runner_id = $2::uuid
    \\  AND l.status = $3 AND l.lease_expires_at > $4
;

/// The write-mint approval check: the newest repository-write gate parked for
/// this fleet+event. The mint requires it approved, its `stated_binding` to
/// match the fleet's CURRENT binding — a `fleet:write` PATCH between the
/// human's answer and this mint is refused as drift — and the answer to have
/// landed inside the card's own deadline.
///
/// The kind is a WHERE clause rather than a post-hoc check: gates of other
/// kinds share the event id, so an install-time grant card raised after the
/// write card would otherwise become "the newest gate" and shadow an answer a
/// human already gave. `id DESC` settles a same-millisecond `created_at` tie,
/// which a re-park after a lost Redis ref can produce.
pub const SELECT_WRITE_GATE_FOR_MINT =
    \\SELECT id::text, status, stated_binding::text, timeout_at, updated_at,
    \\       spend_count, spend_ceiling
    \\FROM core.fleet_approval_gates
    \\WHERE fleet_id = $1::uuid AND event_id = $2 AND gate_kind = $3
    \\ORDER BY created_at DESC, id DESC
    \\LIMIT 1
    \\FOR UPDATE
;

pub const SPEND_WRITE_GATE_FOR_MINT =
    \\UPDATE core.fleet_approval_gates
    \\SET spend_count = spend_count + 1
    \\WHERE id = $1::uuid AND status = $2
    \\  AND spend_count IS NOT NULL AND spend_ceiling IS NOT NULL
    \\  AND spend_count < spend_ceiling
;

/// Heartbeat: bump liveness, and emit a `runner_online` event only on a real
/// transition.
///
/// `FOR UPDATE` serialises concurrent heartbeats from the same host so the
/// pre-bump `last_seen_at` the event tests is the true previous value. The
/// trailing WHERE is what keeps the event stream quiet: an event lands only
/// when the runner was never seen, or was stale past the threshold — a steady
/// heartbeat writes liveness without writing history.
pub const HEARTBEAT_WITH_TRANSITION_EVENT =
    \\WITH locked AS (
    \\  SELECT id, last_seen_at FROM fleet.runners WHERE id = $1::uuid FOR UPDATE
    \\), bumped AS (
    \\  UPDATE fleet.runners r
    \\  SET last_seen_at = $2::bigint, updated_at = $2::bigint
    \\  FROM locked
    \\  WHERE r.id = locked.id
    \\  RETURNING locked.last_seen_at
    \\)
    \\INSERT INTO fleet.runner_events
    \\  (id, runner_id, event_type, metadata, dedup_key, created_at)
    \\SELECT $3::uuid, $1::uuid, $4::text,
    \\       jsonb_build_object($5::text, last_seen_at), NULL, $2::bigint
    \\FROM bumped
    \\WHERE last_seen_at = $6::bigint OR ($2::bigint - last_seen_at) > $7::bigint
;

/// Liveness-only bump, for the paths that must not emit history.
pub const TOUCH_RUNNER_LAST_SEEN =
    \\UPDATE fleet.runners SET last_seen_at = $2, updated_at = $2 WHERE id = $1::uuid
;

// ── Memory fencing ──────────────────────────────────────────────────────────
// Both reads answer "what fencing token is currently valid for this runner on
// this fleet". `COALESCE(a.fencing_seq, l.fencing_token)` prefers the affinity
// slot's sequence, which advances on every re-claim, and falls back to the
// lease's own token when no slot row exists — so a superseded holder presenting
// an old token is rejected either way.

/// Newest live lease for a (runner, fleet) pair.
pub const SELECT_LIVE_FENCE_BY_FLEET =
    \\SELECT COALESCE(a.fencing_seq, l.fencing_token) AS live_seq
    \\FROM fleet.runner_leases l
    \\LEFT JOIN fleet.runner_affinity a ON a.fleet_id = l.fleet_id
    \\WHERE l.runner_id = $1::uuid AND l.fleet_id = $2::uuid
    \\  AND l.status = $3 AND l.lease_expires_at > $4
    \\ORDER BY l.created_at DESC
    \\LIMIT 1
;

/// The same fence, addressed by lease id when the caller already holds one.
pub const SELECT_LIVE_FENCE_BY_LEASE =
    \\SELECT COALESCE(a.fencing_seq, l.fencing_token) AS live_seq
    \\FROM fleet.runner_leases l
    \\LEFT JOIN fleet.runner_affinity a ON a.fleet_id = l.fleet_id
    \\WHERE l.id = $1::uuid AND l.runner_id = $2::uuid AND l.fleet_id = $3::uuid
    \\  AND l.status = $4 AND l.lease_expires_at > $5
    \\LIMIT 1
;
