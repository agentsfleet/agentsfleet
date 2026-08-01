# Register Upstash QStash

**Owners:** 🤠 Indy for Upstash settings and 1Password; 🦉 Orly for secret sync
and verification
**Updated:** Jul 31, 2026
**Prerequisite:** the target environment's admin bootstrap is complete, its API
host passes `/readyz`, and its public schedule ingress is reachable

Use separate QStash credentials for development and production. Complete
development acceptance before production.

| Environment | Destination |
|---|---|
| Development | `https://api-dev.agentsfleet.net/v1/ingress/qstash/schedules` |
| Production | `https://api.agentsfleet.net/v1/ingress/qstash/schedules` |

## 1. Indy: vault the QStash fields

From one Upstash account and region, copy the token, both signing keys, and the
API base shown for that account. In the matching 1Password vault, create or
update `qstash` with:

- `token`.
- `current-signing-key`.
- `next-signing-key`.
- `url` — the HTTPS QStash API base for the same account and region.

Do not infer a regional hostname or mix a token and keys from different
accounts. Use the 1Password application; never paste a value into chat, a
ticket, or a shell command.

QStash deliveries carry a JSON Web Token (JWT) that `agentsfleetd` verifies
against the current and next keys. Current provider behavior is documented in
[QStash signing keys](https://upstash.com/docs/qstash/api-reference/signing-keys/get-signing-keys)
and [QStash schedules](https://upstash.com/docs/qstash/features/schedules).

## 2. Orly: sync the platform bag

After Indy approves the target, run:

```bash
ENV=dev \
ALLOW_VAULT_READS=1 \
ALLOW_PLATFORM_SECRET_WRITES=1 \
  ./playbooks/lib/platform_secret_sync.sh qstash
```

Change `ENV` to `prod` only for the production run. QStash credentials load
when `agentsfleetd` starts, so rerun the matching founding deployment after all
provider bags are synced.

## 3. Indy and Orly: prove the live path

1. Indy creates a disposable scheduled fleet through the target dashboard.
2. Orly runs schedule sync and confirms the provider destination is the exact
   environment URL above.
3. Orly confirms status becomes `desired_status=active` and
   `sync_status=synced` for the current generation.
4. Orly observes one signed delivery and confirms a bad signature and replay
   create no fleet event.
5. Indy removes the disposable schedule; Orly confirms the provider schedule
   is gone.

## Rotation

1. Sync the provider's current and next keys to 1Password and rerun step 2.
2. Restart `agentsfleetd`.
3. Rotate once in Upstash.
4. Copy the newly reported current and next keys, rerun step 2, and restart
   again.

Never rotate twice before step 4; the daemon must retain overlap with the
provider's active key pair.

## Complete when

- The four-field bag exists and the destination is environment-correct.
- `agentsfleetd` has restarted after the sync.
- Create, sync, delivery, rejection, replay, and removal checks pass.
