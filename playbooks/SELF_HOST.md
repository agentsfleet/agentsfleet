---
type: how-to
audience: operator
product_version: 0.78.0
executable: false
---

# Install agentsfleet on your infrastructure

Give the setup agent this page at the revision you intend to install. Follow it through a saved fleet’s first reply. On a repeat run, read the evidence from the earlier run and resume at the first missing check.

For an existing, migrated installation, start at [PRIME.md](./PRIME.md). For agentsfleet.net, use [Hosted operations](./hosted/README.md).

## 1. Record the target

This profile places the dashboard, daemon, Postgres and a three-node Core NATS cluster on one Linux server. Run the runner on a separate Debian 13 host with a private route to the server. Clerk provides sign-in and a model provider serves inference.

Each deployment stage below has a check to record before proceeding.

| Record | Required input |
| --- | --- |
| Software | Checkout revision, immutable daemon image, CLI version, matching runner archive and signed toolbox set |
| Hosts | Linux server with Docker Compose v2 and Caddy, Debian 13 runner host, host architecture for each artifact |
| Origins | HTTPS dashboard and API domains, private runner listener IP and DNS name |
| Owners | Clerk project administrator, model provider administrator, deployment operator, backup destination |
| Capabilities | Setup and runtime chat, plus any optional capabilities from the [operations index](./operations/README.md#optional-capabilities) |

Use the release workflow outputs for the daemon image provenance, runner archive and signed toolbox. The runner verifies the toolbox against its embedded release public key. Keep the archive and toolbox set from the same release.

[Release workflow](../.github/workflows/release.yml)

The checkout containing this guide also contains the CLI and dashboard source. Use the repository's Bun version. Prepare the dashboard checkout at `/srv/agentsfleet` if using the supplied systemd unit.

[Bun version](../package.json)

This profile has one database host. Record the owner and backup storage before admitting real work.

**Evidence** Record the deployment label, four software revisions, daemon digest, release evidence URL, origins, runner host name and enabled capabilities. Store database and provider credentials separately from this record.

## 2. Configure identity, keys and private transport

1. Set up a Clerk project for the dashboard origin. Configure the session token for the API origin and Clerk issuer as described in [Authentication](../docs/AUTH.md). Register the signed `user.created` webhook at `<API_ORIGIN>/v1/auth/identity-events/clerk`.

   Save the signing secret with the backend key from that same Clerk project.
2. Copy [deployment.env.example](../deploy/self-host/deployment.env.example) and [dashboard.env.example](../deploy/self-host/dashboard.env.example) into a private configuration directory. Use the [key inventory](./PRIME.md#where-the-keys-come-from).

   Generate independent hex values with `openssl rand -hex 32` for both database passwords, the NATS password, the device-code pepper and the encryption master key. Place the fields directly into the corresponding credential store or owner-only files. Leave `PLATFORM_ADMIN_WORKSPACE_ID` empty until the signed signup.
3. Issue a TLS leaf certificate with the private runner DNS name in its subject alternative names. Put its chain and private key in `RUNNER_TLS_DIR` as `cert.pem` and `key.pem`. The daemon image runs as UID 65532.

   Grant that UID read access to the mounted directory and leaf files, with `0400` permissions on the key. Keep the CA certificate available for the runner. Restrict TCP and UDP port 4443 on the private listener IP to runner hosts.
4. Configure DNS and the HTTPS proxy using the [Caddyfile](../deploy/self-host/Caddyfile). The public app and API proxy to host loopback. Postgres and NATS have no published host ports.

   Keep the two HTTPS origins and the listener address in the deployment record.

**Evidence** Check that the dashboard publishable key, backend key, webhook and issuer are for one Clerk project. Check the runner certificate against its CA, DNS name and expiration. Confirm the private listener IP belongs to the control-plane host.

[Runner TLS configuration](./operations/runner_stream_tls/001_playbook.md)

## 3. Migrate and start the control plane

Run from the recorded checkout. Define the deployment command once in the operator shell, with an explicit name and absolute configuration paths. Continue to use it from [priming](./PRIME.md) and [recovery](./operations/recovery/README.md).

```bash
af_compose() {
  docker compose --project-name "${AF_INSTALLATION:?set the installation name}" \
    --env-file "${AF_CONFIG_DIR:?set the configuration directory}/deployment.env" \
    -f deploy/self-host/compose.yaml "$@"
}
```

Check the resolved Compose configuration. Success is silent.

```bash
af_compose config --quiet
```

Start the database and three bus peers. Postgres should become healthy, and all three NATS services should run. The readiness probe verifies the bus seeds.

```bash
af_compose up -d --wait postgres nats-a nats-b nats-c
```

Apply the database migrations. The migration container should exit zero.

```bash
af_compose run --rm migrate
```

On a fresh database, create the separate API login once. Open Postgres on the control-plane host with `af_compose exec postgres psql -U agentsfleet_owner -d agentsfleet`. Check `\du agentsfleet_api`.

If absent, run the following and enter the value of `POSTGRES_API_PASSWORD` at the password prompt. Restore procedures recreate the login before restoring its grants.

```sql
CREATE ROLE agentsfleet_api LOGIN IN ROLE api_runtime;
ALTER ROLE agentsfleet_api SET lock_timeout = '2000ms';
ALTER ROLE agentsfleet_api SET statement_timeout = '5000ms';
\password agentsfleet_api
```

Start the daemon. Compose should report the daemon container running.

```bash
af_compose up -d daemon
```

Wait for readiness. The API should answer HTTP 200.

```bash
curl --fail --silent --show-error --retry 10 --retry-delay 1 --retry-all-errors \
  http://127.0.0.1:3000/readyz
```

Check liveness. The API should answer HTTP 200 with an OK status.

```bash
curl --fail --silent --show-error http://127.0.0.1:3000/healthz
```

Use the daemon log to resolve a failed boot. A Postgres startup failure calls for checking the API role and its grants. For a NATS failure, check the three authenticated bus seeds.

Check the leaf files if the runner listener cannot start. Check schema history before retrying a failed migration. Keep the Postgres volume across restarts.

[Migration recovery](./operations/failed_migration/001_playbook.md)

**Evidence** Record migration exit status, installed revision and both probe results. Validate the public API origin from a separate client once the HTTPS proxy is up.

## 4. Build the dashboard and CLI

Install from the repository lockfiles. The dashboard compiles its public environment values into the build. Build as the operator with access to the private configuration directory.

1. Run `bun install --frozen-lockfile` in the checkout and then in `ui/packages/app` and `cli`. Each install should exit zero.
2. In `ui/packages/app`, run `AGENTSFLEET_UI_ENV_FILE="$AF_CONFIG_DIR/dashboard.env" bun run build`. Expect Next.js to finish compiling and list the dashboard routes.
3. In `cli`, run `bun run build`. Expect `dist/bin/agentsfleet.js` to exist.

Create the `agentsfleet-web` system user and group before enabling the [dashboard service](../deploy/self-host/agentsfleet-dashboard.service). Install the same dashboard environment at `/etc/agentsfleet/dashboard.env` with owner `root`, group `agentsfleet-web`, and mode `0640`. When the file already lives there, set its ownership and mode in place.

Give the service user permission to traverse the directory and read the file.

Set the unit's checkout and Bun paths for this host. Confirm the service user can read the build and write to `ui/packages/app/.next/cache`. Install the unit under `/etc/systemd/system`, run `sudo systemctl daemon-reload`, and run `sudo systemctl enable --now agentsfleet-dashboard`.

Place the matching CLI release on the operator's PATH or run `bun cli/dist/bin/agentsfleet.js`.

**Evidence** Open the configured dashboard origin and follow its sign-in page. Record the CLI version, the target API, and the initial dashboard status. Run [PRIME.md](./PRIME.md) next.

## 5. Prove the first fleet

1. As the primed operator, open **New fleet** in the dashboard. Ask for a fleet that formats supplied release notes into a checklist. Choose an eligible setup model and wait for an assistant reply.

   Inspect the setup and its access. Give the saved fleet the display name **Release checklist**, using the deployment's naming rules for its instance identifier.
2. Save the reviewed setup and open the fleet. Send a short sample release note and wait for a complete streamed response. Refresh in a fresh browser session to prove the saved turn is present.

   Take the runner ID from priming or look it up with `agentsfleet platform runners list`. Confirm assignment with `agentsfleet platform runners runs <RUNNER_ID> --fleet <FLEET_ID> --json`.
3. In the CLI with the same account and workspace, check `agentsfleet doctor`, `agentsfleet chat list --json`, `agentsfleet fleet list --json` and `agentsfleet platform runners show <RUNNER_ID>`. Use the chat list to cross-check the setup draft, and the fleet list to identify the saved fleet. If you rerun this check, reuse the existing fixture and send a new sample note.

**Evidence** Save the workspace, draft, fleet, turn and runner IDs, along with acceptance, assignment, first visible reply and completion times where observed. Record live provider and runner checks separately from health and readiness. Keep the fixture for future tests.

## 6. Hand off and resume

Record each check as passed, failed, or skipped and include a command result, request ID or operator evidence. After restarting the dashboard and daemon, confirm the saved fleet and a new reply. Follow [recovery](./operations/recovery/README.md) for a backup and isolated restore, upgrade, and key rotation.

The standard hosted release and teardown procedures live in [Hosted operations](./hosted/README.md).

## Verification on this revision

The Compose profile was exercised on Linux x86-64 with Docker 28.4, Compose 2.40, Postgres 19 and NATS 2.15.3. A source-built daemon applied 56 migrations, passed both API probes and returned to readiness after restart. The catalogue applied 20 identities and 26 prices.

A repeat diff was empty, and a fresh restore preserved the counts and migration history.

Complete the identity, signed runner and provider checks on the intended deployment to certify a live setup and runtime reply. [First fleet verification](#5-prove-the-first-fleet)
