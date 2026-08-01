# Bootstrap the Production Runners

**Owner:** Human
**Executors:** Human prepares external state; Agent prepares hosts; Pipeline
deploys and verifies runners in step 09
**Prerequisite:** step 07 completed; `RELEASE_TAG` is available

## Execution and verification

| Order | Executor | Action | Verifier | Required evidence |
|---|---|---|---|---|
| 1 | Human | Reinstall every host, join Tailscale, and record inventory. | Agent | Every tagged host is online and inventory is valid JSON. |
| 2 | Agent | Prepare packages and directories on every host after Human approval. | `prepare.sh` | Every host preparation exits zero without deploying a runner. |
| 3 | Human and Agent | Bootstrap the administrator; register and synchronize providers. | Provider playbooks | Every provider's post-sync check passes. |
| 4 | Agent | Restart the control plane with runner rollout disabled. | Pipeline | Green release rerun URL. |
| 5 | Human | Add each runner and store every one-time token. | Agent | Required vault fields exist without printing values. |
| 6 | Agent | Enable production runner rollout. | GitHub variable query | `PROD_WORKER_READY=true`. |

## 1. Human and Agent: prepare every host

For each production host, use the provider console to install Debian 13, create
the deploy user, and join Tailscale:

```bash
sudo tailscale up \
  --auth-key "<one-time-key>" \
  --advertise-tags=tag:worker \
  --hostname "<tailscale-hostname>" \
  --ssh
```

Store `tailscale-hostname` and `deploy-user` under that host's production vault
item. Do not create `runner-token` yet.

Set the repository variable to the exact host inventory:

```json
[
  {
    "vault_key": "runner-1"
  },
  {
    "vault_key": "runner-2"
  }
]
```

```bash
gh variable set PROD_WORKER_HOSTS \
  --body "$PROD_WORKER_HOSTS" \
  --repo agentsfleet/agentsfleet
```

After the Human approves changes to the freshly reinstalled hosts, the Agent
runs:

```bash
ALLOW_VAULT_READS=1 \
ALLOW_RUNNER_HOST_PREPARE=1 \
  ./playbooks/founding/08_runner_bootstrap_prod/prepare.sh
```

This prepares each inventory host without installing a runner binary, writing a
token, or starting a service.

## 2. Human and Agent: bootstrap the administrator and providers

The Agent follows `playbooks/operations/admin_bootstrap/001_playbook.md` with
`ENV=prod`.

Register and sync the production applications in the same order used for
development:

1. `playbooks/operations/github_app_registration/001_playbook.md`
2. `playbooks/operations/slack_app_registration/001_playbook.md`
3. `playbooks/operations/zoho_app_registration/001_playbook.md`
4. `playbooks/operations/jira_app_registration/001_playbook.md`
5. `playbooks/operations/linear_app_registration/001_playbook.md`
6. `playbooks/operations/qstash_registration/001_playbook.md`

Complete each registration and sync section. Leave runner-dependent live
acceptance open for step 09. Restart `agentsfleetd` by rerunning the original
release workflow while `PROD_WORKER_READY=false`:

```bash
gh run rerun "$PROD_RELEASE_RUN_ID" \
  --repo agentsfleet/agentsfleet
gh run watch "$PROD_RELEASE_RUN_ID" \
  --repo agentsfleet/agentsfleet \
  --exit-status
```

## 3. Human: create the real runner tokens

Open `https://app.agentsfleet.net`, add one runner per inventory entry, and
store each once-revealed `agt_r` token in the matching production vault item.
The host identifier must equal `tailscale-hostname`; the sandbox tier is
`landlock_full`.

No placeholder token is stored. If a one-time value is lost, revoke that runner
and create it again.

## 4. Agent: enable the Pipeline rollout

```bash
gh variable set PROD_WORKER_READY \
  --body "true" \
  --repo agentsfleet/agentsfleet
```

Step 09 reruns the release. The Pipeline deploys the checked release artifact to
the canary, verifies it, waits for Human approval, and then deploys the remaining
inventory. There is no separate workstation deployment.

Continue to `playbooks/founding/09_verify_prod/001_playbook.md`.
