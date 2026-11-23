---
type: how-to
audience: operator
product_version: 0.78.0
executable: false
---

# Recovery and upgrades

Use the [recorded installation and Compose command](../../SELF_HOST.md#3-migrate-and-start-the-control-plane). Keep each backup with the installed revision, capture time and secret-store references needed to recover it.

## Back up Postgres

Use an owner-only backup destination and a new archive path. Store the archive encrypted off the database host. Retain the daemon's encryption master key separately in the deployment's credential store.

A database dump alone cannot recover encrypted workspace secrets.

```bash
(umask 077; set -o noclobber
 af_compose exec -T postgres pg_dump -U agentsfleet_owner -d agentsfleet -Fc > "$AF_BACKUP_FILE")
```

Expect a zero exit and a new archive. List its catalogue.

```bash
af_compose exec -T postgres pg_restore --list < "$AF_BACKUP_FILE"
```

Expect a table of contents. Record the archive checksum.

```bash
sha256sum "$AF_BACKUP_FILE"
```

Only accept the archive after a successful dump, readable catalogue and checksum. Record the readback without including credential or prompt contents. Postgres contains the durable state.

Core NATS comes back as an empty transport and reconnects to the restored daemon.

## Restore into a separate installation

1. Select a separate Compose project, new volume and recovery-only identity and ingress endpoints. Keep the original installation running independently. Load the backup's master key in the isolated daemon configuration and leave the runner stopped.

   Check the archive checksum and use the same Postgres major version.
2. Start only Postgres. Create the shared roles from the [release's roles migration](../../../rustd/crates/daemon/afd_db/schema/20261108120100_roles_and_privileges.sql) using the names below. Open the same `psql` command as in [control-plane setup](../../SELF_HOST.md#3-migrate-and-start-the-control-plane).

   ```sql
   CREATE ROLE db_migrator NOLOGIN;
   CREATE ROLE api_runtime NOLOGIN;
   CREATE ROLE memory_runtime NOLOGIN;
   CREATE ROLE ops_readonly_human NOLOGIN;
   CREATE ROLE ops_readonly_fleet NOLOGIN;
   GRANT memory_runtime TO api_runtime WITH INHERIT FALSE, SET TRUE;
   ```

3. Create the API login and its timeout settings from control-plane setup. Enter the password for the isolated environment. Restore only into the empty database.

   ```bash
   af_compose exec -T postgres pg_restore --exit-on-error --single-transaction \
     -U agentsfleet_owner -d agentsfleet < "$AF_BACKUP_FILE"
   ```

   Expect a zero exit from the restore.
4. Run `af_compose run --rm migrate`. Expect a zero exit and leave the runner stopped.
5. Verify migration history, a known workspace and saved fleet, and the daemon's readiness. For a live verification, use recovery-only provider credentials, webhook routes and a separately registered runner. Confirm inference can decode a stored secret without printing it.

   Record the checksum, restore time, revision and identifiers. Resume [priming](../../PRIME.md) at the first missing check.

## Upgrade or roll back

1. Record current versions and digests, take a verified backup, and drain the runner before maintenance. Use [runner onboarding](../runner_onboarding/001_playbook.md) for runner replacement.
2. In an isolated installation, restore the backup and migrate with the candidate daemon image. Verify readiness, dashboard and CLI sign-in, the saved fixture, and its first setup and runtime replies with the matching candidate runner and signed toolbox.
3. After approval for the target installation, deploy the candidate image, dashboard, CLI and runner together. Record migration and verification evidence before admitting new work.
4. For a startup problem after migration, compare schema history with the prior binary and use the [failed migration guide](../failed_migration/001_playbook.md). For an incompatible schema, recover from the last matching backup into an isolated target before switching ingress. Record any writes that would be lost since that backup before approving a cutover.

## Rotate credentials by issuer

| Credential | Recovery |
| --- | --- |
| Provider inference key | Issue the new value at the provider. Replace the named secret with `secrets update`. Verify a paid request through the same model, then revoke the old key. |
| CLI session or tenant API key | Revoke the affected session or API key and grant a new one. Update and verify each consumer. |
| Runner token | Drain or stop the intended host. Use `platform runners rotate-token <ID> --token-out <FILE>`, replace its stored service value and verify the heartbeat and self-test. |
| Clerk webhook secret or backend key | Coordinate the change in the same Clerk project and relevant services. Verify signed webhook delivery, browser sign-in and CLI sign-in. |
| Database or NATS | Update the issuing service and its consumers together during maintenance. Check migration and API connectivity, then a real turn. |
| Runner TLS | Install the new trust chain before changing the leaf. Reconnect and verify runner heartbeats over the private endpoint. |
| Encryption key or device-code pepper | Retain the original encryption key for any archive that depends on it. Use a tested data migration for an encryption-key change. Changing the device-code pepper invalidates outstanding device-code attempts. |

## Identity recovery

A missing account after a fresh database build needs the verified Clerk user's signed `user.created` delivery. Check its result and the metadata writeback. A deleted Clerk user remains in application storage because the signed identity handler acknowledges `user.deleted` without erasing data.

The product currently has no per-user purge. In the authorized database console, look up the identity using its Clerk user identifier and a bound parameter.

```sql
SELECT id, tenant_id FROM identity.users WHERE oidc_subject = $1;
```

Record whether the application identity exists. Escalate an erasure request through the operator responsible for application data and retention. [Hosted database teardown](../teardown/database/001_playbook.md) applies to an approved whole-database rebuild.
