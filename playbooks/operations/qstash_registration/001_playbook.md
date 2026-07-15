# M105_001: Playbook — Register Upstash QStash for Fleet schedules

**Milestone:** M105
**Workstream:** 001 (§8.1 deliverable)
**Updated:** Jul 15, 2026
**Prerequisite:** `op` Command-Line Interface (CLI) authenticated; the `agentsfleet-admin` tenant API key in vault (`operations/admin_bootstrap/001_playbook.md`); an Upstash account with QStash enabled; the target `agentsfleetd` API deployed and reachable.

Registers the environment's Upstash QStash credentials for hosted Fleet schedules. QStash owns the clock; `agentsfleetd` owns schedule state, synchronous schedule mutation calls, and the signed ingress at `/v1/ingress/qstash/schedules`. The daemon loads one admin-workspace vault item named `qstash` with fields `{token,current_signing_key,next_signing_key}`.

> **Run once per environment.** Re-running §4 with `--force` is idempotent on the vault item. QStash key rotation uses the same playbook: update `next_signing_key`, roll once in Upstash, then update both fields before rolling again.

---

## Human vs Agent Split

| Step | Owner | What |
|------|-------|------|
| 0.0 | Agent | Resolve environment and verify the public ingress URL is reachable |
| 1.0 | Human | Open the Upstash QStash console and copy the API token plus signing keys |
| 2.0 | Human | Confirm the destination URL exactly matches the deployed API base |
| 3.0 | Agent | Store `{token,current_signing_key,next_signing_key}` as `qstash` in the `agentsfleet-admin` vault |
| 4.0 | Agent | Verify metadata, restart or roll `agentsfleetd`, and run one schedule sync smoke test |

Steps 1–2 are browser-interactive; 3–4 run unattended once the values exist in 1Password or are pasted at local prompts.

---

## 0.0 Agent: Resolve environment and ingress

**Goal:** pick dev or prod, read the admin key, and prove the public ingress host is live before storing provider credentials.

```bash
export ENV="prod"   # or: export ENV="dev"
case "$ENV" in
  dev)  export VAULT="ZMB_CD_DEV";  export API_BASE="https://api-dev.agentsfleet.net" ;;
  prod) export VAULT="ZMB_CD_PROD"; export API_BASE="https://api.agentsfleet.net" ;;
  *)    echo "ENV must be dev|prod"; exit 1 ;;
esac

export QSTASH_DESTINATION="$API_BASE/v1/ingress/qstash/schedules"
export ADMIN_KEY=$(op read "op://$VAULT/agentsfleet-admin/api_key")
[[ "$ADMIN_KEY" =~ ^agt_t[0-9a-f]{64}$ ]] || { echo "missing admin key"; exit 1; }
curl -sf -o /dev/null "$API_BASE/healthz" || { echo "$API_BASE unreachable"; exit 1; }
```

### Acceptance

`$API_BASE/healthz` returns 200; `QSTASH_DESTINATION` is the exact public URL QStash will sign in deliveries.

---

## 1.0 Human: Copy QStash credentials from Upstash

**Goal:** capture three values from the Upstash QStash console:

- QStash API token
- current signing key
- next signing key

Upstash uses the API token in the `Authorization: Bearer …` header for schedule create/update/delete calls. QStash deliveries carry an `Upstash-Signature` JSON Web Token (JWT); `agentsfleetd` verifies it against the current and next signing keys so one key roll can happen without downtime.

### Acceptance

All three values are available to the operator and are not pasted into chat, tickets, shell history, or command arguments.

---

## 2.0 Human: Confirm destination URL

**Goal:** ensure every schedule that agentsfleet creates points to:

```text
https://api.agentsfleet.net/v1/ingress/qstash/schedules
```

Use the dev hostname for dev. The URL matters because QStash signs the destination as part of the delivery verification input; a proxy, scheme, or hostname mismatch causes `UZ-SCHED-005`.

### Acceptance

The chosen destination URL exactly matches the deployed `API_BASE` plus `/v1/ingress/qstash/schedules`.

---

## 3.0 Agent: Store the admin-workspace vault item

**Goal:** persist the QStash secret bag as `qstash` in the `agentsfleet-admin` workspace vault. Prefer writing values to 1Password first, then piping `op read` into the API so secret bytes never appear in argv.

```bash
qstash_token=$(op read "op://$VAULT/qstash/token" 2>/dev/null) || read -rsp 'qstash token: ' qstash_token >&2
qstash_current=$(op read "op://$VAULT/qstash/current_signing_key" 2>/dev/null) || read -rsp 'current signing key: ' qstash_current >&2
qstash_next=$(op read "op://$VAULT/qstash/next_signing_key" 2>/dev/null) || read -rsp 'next signing key: ' qstash_next >&2

printf '%s\0%s\0%s' "$qstash_token" "$qstash_current" "$qstash_next" |
  jq -Rs 'split("\u0000") | {
    token: .[0],
    current_signing_key: .[1],
    next_signing_key: .[2]
  }' |
  AGENTSFLEET_API_KEY="$ADMIN_KEY" agentsfleet secret create qstash --force --data @-

unset qstash_token qstash_current qstash_next
```

### Acceptance

`agentsfleet secret create` exits 0. No token or signing key appears in command output, shell history, or process arguments.

---

## 4.0 Agent: Verify and roll `agentsfleetd`

**Goal:** verify the vault item exists, then restart or roll the daemon so process-lifetime QStash credentials load at boot.

```bash
AGENTSFLEET_API_KEY="$ADMIN_KEY" agentsfleet secret show qstash --json | jq '{name,kind}'

# Roll/restart agentsfleetd for the target environment using the normal deploy path.
# Then run one explicit sync against a known test Fleet schedule:
AGENTSFLEET_API_KEY="$ADMIN_KEY" agentsfleet schedule sync "$FLEET_ID" "$SCHEDULE_ID"
AGENTSFLEET_API_KEY="$ADMIN_KEY" agentsfleet schedule status "$FLEET_ID" "$SCHEDULE_ID"
```

### Acceptance

`secret show` returns metadata only; after the daemon roll, `schedule sync` reaches QStash and `schedule status` reports `sync_status=active` for the current generation.

---

## Rotation

1. In Upstash QStash, generate or copy the **next** signing key and store it in `op://$VAULT/qstash/next_signing_key`.
2. Re-run §3 so `agentsfleetd` trusts both current and next.
3. Roll `agentsfleetd`.
4. In Upstash, roll signing keys once. Upstash promotes `next` to `current` and creates a new `next`.
5. Update both 1Password fields from Upstash, re-run §3, and roll `agentsfleetd` again.

Do not roll twice before §5 completes. After two provider-side rolls, the daemon's old current/next pair can no longer verify new deliveries.

---

## Rollback

1. Pause or delete affected Fleet schedules with `agentsfleet schedule update … --status paused` or `agentsfleet schedule rm …`.
2. Delete or replace the `qstash` vault item only after schedules are inert.
3. Roll `agentsfleetd`; schedule management then fails closed with `UZ-SCHED-007` until §3 is completed again.
