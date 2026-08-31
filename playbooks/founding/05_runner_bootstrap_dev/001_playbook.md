# Bootstrap the Development Runner

**Owner:** Human
**Executors:** Human prepares external state; Agent prepares the host; Pipeline
deploys and verifies the runner
**Prerequisite:** step 04 completed and `DEV_DEPLOY_RUN_ID` is available

## Execution and verification

| Order | Executor | Action | Verifier | Required evidence |
|---|---|---|---|---|
| 1 | Human | Reinstall the runner host and join it to Tailscale. | Agent | Tailscale reports the expected tagged host online. |
| 2 | Agent | Prepare host packages and directories after Human approval. | `prepare.sh` | Preparation exits zero without installing a runner binary. |
| 3 | Human and Agent | Bootstrap the administrator; register and synchronize providers. | Provider playbooks | Every provider's post-sync check passes. |
| 4 | Agent | Restart the control plane while runner deployment remains disabled. | Pipeline | Green development workflow URL. |
| 5 | Human | Add the runner and store its one-time token. | Agent | Required vault fields exist without printing values. |
| 6 | Agent | Enable runner deployment and start a new development workflow. | Pipeline | Runner deployment, cgroup verification, and development acceptance are green. |

## 1. Human: prepare the host

Use the provider console for the first login. Install Debian 13, create the
non-root deploy user, install Tailscale, and join with:

```bash
sudo tailscale up \
  --auth-key "<one-time-key>" \
  --advertise-tags=tag:worker \
  --hostname "agentsfleet-dev-runner-ant" \
  --ssh
```

Store these fields under
`op://ZMB_CD_DEV/agentsfleet-dev-runner-ant`:

- `tailscale-hostname`
- `deploy-user`

The tailnet policy from step 02 must already allow `tag:ci` to reach
`tag:worker`. Tailscale Secure Shell (SSH) distributes and checks host keys; the
automation never disables host-key verification.

After the Human approves this fresh-host mutation, the Agent runs:

```bash
ALLOW_VAULT_READS=1 \
ALLOW_RUNNER_HOST_PREPARE=1 \
  ./playbooks/founding/05_runner_bootstrap_dev/prepare.sh
```

Before changing the host, preparation checks that cgroup v2 exposes Central
Processing Unit (CPU), memory, and process controllers. The deployment workflow
then verifies that systemd delegated those controllers to the started runner
service. Preparation installs host dependencies and creates deployment
directories; it does not copy a runner binary, write a token, or start a
service.

## 2. Human and Agent: bootstrap the administrator and providers

Follow `playbooks/operations/admin_bootstrap/001_playbook.md` with `ENV=dev`.
The Human completes dashboard signup and provider-console actions.

Register and sync every supported development provider in this order:

1. `playbooks/operations/github_app_registration/001_playbook.md`
2. `playbooks/operations/slack_app_registration/001_playbook.md`
3. `playbooks/operations/zoho_app_registration/001_playbook.md`
4. `playbooks/operations/jira_app_registration/001_playbook.md`
5. `playbooks/operations/linear_app_registration/001_playbook.md`
6. `playbooks/operations/qstash_registration/001_playbook.md`

Complete each registration and sync section now. Leave runner-dependent live
acceptance open for step 06.

Restart through the normal development workflow while the runner lane remains
disabled, and retain the new run identifier:

```bash
gh workflow run deploy-dev.yml \
  --ref main \
  --repo agentsfleet/agentsfleet

DEV_DEPLOY_RUN_ID="$(
  gh run list \
    --repo agentsfleet/agentsfleet \
    --workflow deploy-dev.yml \
    --branch main \
    --limit 1 \
    --json databaseId \
    --jq '.[0].databaseId'
)"
gh run watch "$DEV_DEPLOY_RUN_ID" \
  --repo agentsfleet/agentsfleet \
  --exit-status
```

## 3. Human: create the real runner token

Open `https://app-dev.agentsfleet.net`, select **Add runner**, use the Tailscale
hostname as the host identifier, and give the runner the `landlock_full`
sandbox tier. Only now add the once-revealed token to the existing 1Password
item:

```bash
op item edit \
  "agentsfleet-dev-runner-ant" \
  --vault ZMB_CD_DEV \
  "runner-token[concealed]=<agt_r token>"
```

No placeholder token is stored. If the one-time value is lost, revoke the
runner and create it again.

## 4. Agent and Pipeline: deploy and verify

```bash
gh variable set DEV_RUNNER_READY \
  --body "true" \
  --repo agentsfleet/agentsfleet

gh workflow run deploy-dev.yml \
  --ref main \
  --repo agentsfleet/agentsfleet

DEV_VERIFY_RUN_ID="$(
  gh run list \
    --repo agentsfleet/agentsfleet \
    --workflow deploy-dev.yml \
    --branch main \
    --limit 1 \
    --json databaseId \
    --jq '.[0].databaseId'
)"
gh run watch "$DEV_VERIFY_RUN_ID" \
  --repo agentsfleet/agentsfleet \
  --exit-status
```

The Pipeline downloads its own checked artifact and invokes the canonical runner
deployment and verification scripts. It installs the tracked systemd unit,
writes the environment file without exposing the token, starts the service, and
fails closed unless `cpu`, `memory`, and `pids` are delegated.

Continue to `playbooks/founding/06_verify_dev/001_playbook.md`.
