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

Run this drill on a dedicated recovery host or virtual machine. The Compose daemon binds host ports `3000` and `4443` there. Prepare a separate Compose project with an empty volume and recovery-only identity and ingress endpoints.

1. Stage the recovery images.

   ```bash
   af_compose pull postgres nats-a nats-b nats-c migrate daemon
   ```

   Compose reports the Postgres, NATS, and daemon images available locally.
2. Create the network and a stopped Postgres container.

   ```bash
   af_compose create --pull never postgres
   ```

   Compose reports the Postgres container as created and creates the `${AF_INSTALLATION}_default` network.
3. Verify access to a test endpoint before blocking egress. Set `AF_EGRESS_CHECK_URL` to an operator-controlled HTTP endpoint at a literal IP address outside the recovery network that returns HTTP 200.

   ```bash
   curl --noproxy '*' --max-time 5 --fail --silent --show-error \
     --output /dev/null --write-out '%{http_code}\n' "$AF_EGRESS_CHECK_URL"
   ```

   ```text
   200
   ```

   Run the second probe on the recovery Compose network.

   ```bash
   docker run --rm --pull never --network "${AF_INSTALLATION}_default" --entrypoint wget \
     docker.io/library/postgres:19-alpine -Y off -T 5 -O /dev/null "$AF_EGRESS_CHECK_URL"
   ```

   Expect a successful connection and exit zero.
4. Isolate the recovery host and its containers before copying the archive or starting services. Block traffic to all external networks, including the original installation. Keep local loopback and the Compose network available.

   Repeat both probes and confirm that each fails to connect or times out. Record the results and keep the restriction for the entire drill.

   The restored daemon can dispatch pending connector deliveries as soon as it starts. A backup and its original master key can restore grants that send to live destinations. Use the backup's master key in the isolated daemon configuration and leave the runner stopped.
5. Start Postgres after checking the archive checksum and matching the backup's Postgres major version.

   ```bash
   af_compose up -d --wait --pull never postgres
   ```

   Expect the recovery Postgres container to be healthy.
6. Create the shared roles from the [release's roles migration](../../../rustd/crates/daemon/afd_db/schema/20261108120100_roles_and_privileges.sql) using the names below. Open the same `psql` command as in [control-plane setup](../../SELF_HOST.md#3-migrate-and-start-the-control-plane).

   ```sql
   CREATE ROLE db_migrator NOLOGIN;
   CREATE ROLE api_runtime NOLOGIN;
   CREATE ROLE memory_runtime NOLOGIN;
   CREATE ROLE ops_readonly_human NOLOGIN;
   CREATE ROLE ops_readonly_fleet NOLOGIN;
   GRANT memory_runtime TO api_runtime WITH INHERIT FALSE, SET TRUE;
   ```

7. Create the API login and its timeout settings from control-plane setup. Enter the password for the isolated environment.
8. Restore into the empty database.

   ```bash
   af_compose exec -T postgres pg_restore --exit-on-error --single-transaction \
     -U agentsfleet_owner -d agentsfleet < "$AF_BACKUP_FILE"
   ```

   Expect a zero exit from the restore.
9. Verify the restored schema with the release migrator.

   ```bash
   af_compose run --rm --pull never migrate
   ```

   Expect exit zero. Verify migration history, a known workspace and a saved fleet in the restored database.
10. Start the bus and daemon with the network restriction still in place.

    ```bash
    af_compose up -d --pull never nats-a nats-b nats-c daemon
    ```

    Expect the daemon and all three NATS services to be running.
11. Probe the recovery daemon on the recovery Compose network.

    ```bash
    af_compose exec -T postgres sh -c '
      for attempt in $(seq 10); do
        if wget -Y off -T 5 -O - http://daemon:3000/readyz; then
          exit 0
        fi
        sleep 1
      done
      exit 1
    '
    ```

    Expect HTTP 200 within ten attempts. Record the checksum, restore time, revision and identifiers. Record restored-secret decryption as unverified during this drill.
12. Stop the drill services before releasing the network restriction.

    ```bash
    af_compose stop
    ```

    Expect Compose to report the daemon, bus, and database containers stopped.

Use a fresh fixture installation with its own provider credentials, webhook routes and separately registered runner for live sign-in and inference checks. Use [priming](../../PRIME.md) for that installation.

## Upgrade or roll back

1. Record current versions and digests, take a verified backup, and drain the runner before maintenance. Use [runner onboarding](../runner_onboarding/001_playbook.md) for runner replacement.
2. Restore the backup on the isolated recovery host and migrate with the candidate daemon image. Verify readiness and the saved fleet there. Verify dashboard and CLI sign-in and the first setup and runtime replies on a fresh fixture installation with the matching candidate runner and signed toolbox.
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
