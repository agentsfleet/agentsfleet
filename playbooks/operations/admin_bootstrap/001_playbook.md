---
type: how-to
audience: operator
verified: 2026-11-12
product_version: 0.76.0
executable: false
---

# Bootstrap the platform administrator

## What this does

Opens the administrator account, points the daemon at its workspace, and creates the key the bag syncs use.
Tenant, workspace and Clerk user identifiers are three different values.

## Before you begin

The matching API and dashboard are deployed, migrations are finished and `/readyz` succeeds.
Indy approves vault reads and runtime changes for the environment; production needs its own approval.

```bash
export ENV=dev VAULT=ZMB_CD_DEV FLY_APP=agentsfleetd-dev
export AGENTSFLEET_API_URL=https://api-dev.agentsfleet.net
```

Production uses `ENV=prod`, its own vault, `agentsfleetd-prod` and `https://api.agentsfleet.net`.

## Steps

1. **Open the account.** Sign up at the dashboard with the identity in `agentsfleet-admin`, and verify the email.
   After a database wipe, Indy resends that Clerk user's signed `user.created` event instead; signing in creates no rows.
2. **Add platform scopes.** In the Clerk user's public metadata, keep `tenant_id` and append these to `scopes`:

   ```text
   runner:enroll runner:write stream:read model:admin platform-key:admin platform-library:write workspace:any
   ```

3. **Sign the CLI in.** Run this from an interactive terminal; `login` refuses a shell without one.

   ```bash
   agentsfleet login --force
   agentsfleet whoami
   agentsfleet workspace list
   ```

   `whoami` lists the seven platform scopes, and `workspace list` shows one workspace.
4. **Point the daemon at that workspace.** Save its ID in `agentsfleet-admin/platform_admin_workspace_id`, then:

   ```bash
   flyctl secrets set --app "$FLY_APP" \
     "PLATFORM_ADMIN_WORKSPACE_ID=$(op read "op://$VAULT/agentsfleet-admin/platform_admin_workspace_id")"
   ```

   The daemon restarts; wait for `/readyz`. Never use the Clerk `tenant_id` or `user.id` here.
5. **Create the sync key.** Run `agentsfleet api-keys create --name platform-priming --json`; it reveals the `agt_t` value once.
   Store it in the concealed `api-key` field of `agentsfleet-admin` through a template file, never as a command argument.
   Use `op item get --format json`, set the field, then `op item edit --template <file>`, and delete the file.

## Sync a provider secret bag

```bash
ALLOW_VAULT_READS=1 ALLOW_PLATFORM_SECRET_WRITES=1 ./playbooks/lib/platform_secret_sync.sh <BAG>
```

```text
✓ <BAG> created in dev admin workspace
```

It says `replaced` when the bag exists, and reads `api-key` and `platform_admin_workspace_id` from `agentsfleet-admin`.
No value reaches process arguments or output; an HTTP 403 or 404 means the key's scopes or the workspace ID are wrong.

| Bag | 1Password item and fields | `agentsfleetd` reads it |
|---|---|---|
| `approval-signing` | `approval-signing-secret`: `credential`, stored as `webhook_secret` | Per request |
| `qstash` | `qstash`: `token`, `current-signing-key`, `next-signing-key`, `url` (`https://<host>/v2`) | Per use |
| `smtp-relay` | `smtp-relay`: `host`, `port`, `username`, `password`, `from_address` | Per send |
| `github-app` | `github-app`: `app_id`, `app_slug`, `client_id`, `client_secret`, `private_key_pem`, `webhook_secret` | At startup |
| `slack-app` | `slack-app`: `client_id`, `client_secret`, `signing_secret` | Per request |
| `linear-app`, `jira-app`, `zoho-app` | Same-named item: `client_id`, `client_secret` | At startup |

A startup-read bag takes effect at the next restart, such as the runner redeploy.

## Verify it works

```bash
agentsfleet whoami
agentsfleet secrets list
```

`whoami` lists the platform scopes, and `secret list` shows every synced bag.

## Common problems

| Symptom | Cause | Action |
|---|---|---|
| `login` needs an interactive terminal | The shell has no terminal | Run it in a terminal app |
| `UZ-AUTH-002` from the CLI | The session ended, often after a wipe | Run `agentsfleet login --force` again |
| Platform commands refused | Platform scopes missing | Add them in Clerk; the CLI picks them up within minutes |
| Bag sync returns 401 | `api-key` belongs to a wiped account | Repeat step 5 |

## Remove or undo

Revoke an unused key with `agentsfleet api-keys revoke`; remove platform scopes in Clerk.
Deleting the Clerk user leaves database rows; see [user teardown](../teardown/user/001_playbook.md).

## Related pages

- [Authentication and signup](../../../docs/AUTH.md)
- [Model catalogue priming](../model_catalogue/001_playbook.md)
- [Credential rotation](../credential_rotation/001_playbook.md)
