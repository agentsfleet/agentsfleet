# Add or Replace a Runner

**Owner:** Human
**Executors:** Human creates external state; Agent prepares the host; Pipeline
deploys and verifies the runner

Use this operation after the founding installation. Use the chronological setup
route after a wipe.

| Environment | Dashboard | Vault | Runner inventory |
|---|---|---|---|
| Development | `https://app-dev.agentsfleet.net` | `ZMB_CD_DEV` | `agentsfleet-dev-runner-ant` |
| Production | `https://app.agentsfleet.net` | `ZMB_CD_PROD` | `PROD_RUNNER_HOSTS` |

## Execution and verification

| Order | Executor | Action | Verifier | Required evidence |
|---|---|---|---|---|
| 1 | Human | Install Debian, create the deploy user, and join Tailscale as `tag:worker`. | Agent | Expected Tailscale hostname is online. |
| 2 | Agent | Prepare packages and directories after Human approval. | `prepare.sh` | Preparation exits zero without deploying a binary. |
| 3 | Human | Create the runner in the dashboard and store its one-time token. | Agent | Required vault fields exist without printing values. |
| 4 | Agent | Add the production item to inventory when applicable and start the routine deployment. | Pipeline | Runner deployment and verification jobs are green. |
| 5 | Human | Approve the production fleet after its canary is green. | Pipeline | Remaining production runners deploy sequentially. |

## Prepare the host

The 1Password item must contain `tailscale-hostname` and `deploy-user`. Do not
create a placeholder token.

Development:

```bash
ALLOW_VAULT_READS=1 \
ALLOW_RUNNER_HOST_PREPARE=1 \
  ./playbooks/founding/05_runner_bootstrap_dev/prepare.sh
```

Production:

```bash
ALLOW_VAULT_READS=1 \
ALLOW_RUNNER_HOST_PREPARE=1 \
ENV=prod \
RUNNER_ITEM=<PRODUCTION_RUNNER_ITEM> \
  ./playbooks/lib/runner/prepare.sh
```

Preparation installs host dependencies and creates deployment directories. It
does not install a runner binary, write a token, or start a service.

## Create the runner

The Human signs in to the matching dashboard and opens **Configuration →
Runners → Create runner**.

| Field | Value |
|---|---|
| Host identifier | Exact `tailscale-hostname` from 1Password |
| Sandbox tier | `landlock_full` |
| Labels | `dev` or `prod`, matching the environment |

Store the once-revealed `agt_r` value in the concealed `runner-token` field on
the same 1Password item. If the value is lost, revoke the runner and create it
again.

For production, add the vault item to `PROD_RUNNER_HOSTS` before starting the
deployment.

## Deploy through the Pipeline

- Development follows
  [`deploy/dev/001_playbook.md`](../../deploy/dev/001_playbook.md).
- Production follows
  [`deploy/prod/001_playbook.md`](../../deploy/prod/001_playbook.md).

The Pipeline downloads its checked binary, installs the tracked systemd unit,
starts the service, and runs the host gate. A workstation does not deploy the
binary.

## Complete when

- The vault item has a real one-time token and no placeholder.
- The runner deployment job is green.
- The gate verifies `cpu`, `memory`, and `pids` delegation.
- The dashboard reports the exact host identifier as `online`.
