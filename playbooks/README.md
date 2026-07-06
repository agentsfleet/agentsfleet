# Playbooks

Two tiers, by intent:

- **`founding/`** — the **sequential** spine you run **once, in order**, to stand an agentsfleet platform up from nothing. Run lexically from `01_bootstrap` upward; each step declares its predecessor as a prerequisite.
- **`operations/`** — **on-demand** runbooks with **no implied order**: rotate a credential, set up observability, tear an environment down. Named by what they do, not numbered, because they are not a sequence. Destructive teardowns are isolated under `operations/teardown/`.

> The old flat `001…015` numbering implied a single sequence that did not exist — teardowns, CI image builds, and post-deploy admin setup were numbered as if they followed the deploy steps. This split makes the founding path legible and keeps ops runbooks from masquerading as founding steps.

> **Architecture rationale:** see [`ARCHITECTURE.md`](./ARCHITECTURE.md) for why every public-facing service in this project sits behind a Cloudflare Tunnel — the WHY behind the operational steps in `founding/01_bootstrap`, `founding/03_priming_infra`, and the deploy playbooks.

```
playbooks/
├── README.md                          ← this file
├── ARCHITECTURE.md                    ← architecture rationale (tunnel-first)
├── lib/
│   └── common.sh                      ← shared gate helpers
├── founding/                          ← run ONCE, in order, to stand the platform up
│   ├── 01_bootstrap/                  ← human + agent: accounts, root keys, vault handoff
│   │   ├── 001_playbook.md
│   │   └── 02_vercel_env.sh
│   ├── 02_preflight/                  ← credential gate (CI job 0)
│   │   ├── 001_playbook.md
│   │   ├── 00_gate.sh
│   │   ├── 01_tools_and_auth.sh
│   │   └── 02_credentials.sh
│   ├── 03_priming_infra/              ← provision Fly, Cloudflare tunnel, data plane
│   │   ├── 001_playbook.md
│   │   └── 002_workers_and_handoff.md
│   ├── 04_deploy_dev/
│   │   └── 001_playbook.md
│   ├── 05_deploy_prod/
│   │   └── 001_playbook.md
│   ├── 06_runner_bootstrap_dev/       ← bring up a DEV worker node
│   │   ├── 001_playbook.md
│   │   ├── 00_gate.sh
│   │   ├── 01_ssh_access.sh
│   │   ├── 02_host_readiness.sh
│   │   ├── 03_deploy_readiness.sh
│   │   └── 04_provision_runner_env.sh
│   └── 07_runner_bootstrap_prod/
│       └── 001_playbook.md
└── operations/                        ← on-demand runbooks, NO implied order
    ├── admin_bootstrap/               ← provision the global agentsfleet-admin user
    │   └── 001_playbook.md
    ├── slack_app_registration/        ← register the @agentsfleet Slack app + vault platform secrets
    │   └── 001_playbook.md
    ├── github_app_registration/       ← register the agentsfleet GitHub App + vault the App private key
    │   └── 001_playbook.md
    ├── zoho_app_registration/         ← register the agentsfleet Zoho Desk OAuth app + vault platform secrets
    │   └── 001_playbook.md
    ├── jira_app_registration/         ← register the agentsfleet Jira OAuth (3LO) app + vault platform secrets
    │   └── 001_playbook.md
    ├── linear_app_registration/       ← register the agentsfleet Linear OAuth app + vault platform secrets
    │   └── 001_playbook.md
    ├── credential_rotation/           ← rotate an exposed DEV credential
    │   ├── 001_playbook.md
    │   ├── 00_gate.sh
    │   ├── 01_vault_sync.sh
    │   └── 02_service_health.sh
    ├── runner_onboarding/             ← mint a runner agt_r via the dashboard
    │   └── 001_playbook.md
    ├── observability/                 ← bootstrap the Grafana stack
    │   ├── 001_playbook.md
    │   ├── 002_grafana_setup.md
    │   ├── 00_gate.sh
    │   ├── 01_credentials.sh
    │   ├── 02_prometheus.sh
    │   └── 03_dashboard.sh
    ├── ip_allowlisting/               ← harden the data-plane network boundary
    │   ├── 001_playbook.md
    │   ├── 00_gate.sh
    │   ├── 01_egress_inventory.sh
    │   └── 02_provider_targets.sh
    ├── ci_zig_images/                 ← pre-bake Zig + OpenSSL CI images
    │   ├── 001_playbook.md
    │   ├── build_and_push.sh
    │   ├── Dockerfile.alpine
    │   ├── Dockerfile.debian-trixie
    │   ├── Dockerfile.ubuntu
    │   └── versions.env
    ├── installer_deploy/              ← serve agentsfleet.dev one-URL installer
    │   └── 001_playbook.md
    └── teardown/                      ← DESTRUCTIVE; own approval guards
        ├── database/
        │   ├── 001_playbook.md
        │   ├── 00_gate.sh
        │   ├── 01_credential_check.sh
        │   ├── 02_teardown.sh
        │   ├── 03_verify.sh
        │   └── teardown.sql
        ├── redis/
        │   ├── 001_playbook.md
        │   ├── 00_gate.sh
        │   ├── 01_credential_check.sh
        │   ├── 02_teardown.sh
        │   └── 03_verify.sh
        └── user/                      ← DEV: delete a Clerk user (webhook purges the tenant)
            └── 001_playbook.md
```

> The directory tree above is asserted against disk by `make check-playbooks` — adding or removing a playbook directory without updating this README fails the gate.

## Playbooks vs Gates

**Playbooks** (`playbooks/<tier>/<name>/001_playbook.md`) are human-readable runbooks. They describe:

- Who does what (human vs agent)
- Step-by-step procedures with context and rationale
- Acceptance criteria per step
- Dependencies and prerequisites

Playbooks are documentation. They are NOT executable.

**Gates** (`playbooks/<tier>/<name>/00_gate.sh` + numbered sections) are machine-executable verification scripts. They:

- Validate that a playbook's acceptance criteria are met
- Run in CI as pipeline prerequisites (e.g. `deploy-dev.yml` runs `founding/02_preflight/00_gate.sh` as job 0)
- Run locally by agents to verify state before proceeding
- Exit non-zero on any failure — fail loud, fail all items (not just the first)

Gates are executable. They are NOT documentation.

## Gate Script Convention

Each gate lives inside its playbook directory.

- `00_gate.sh` — dispatcher. Globs `01_*.sh`, `02_*.sh`, etc. and runs them in order.
- `01_name.sh`, `02_name.sh` — numbered section scripts. Two-digit prefix, descriptive snake_case name.
- All scripts are `set -euo pipefail`, print per check, exit 1 if any check fails.
- Environment: `VAULT_DEV`, `VAULT_PROD`, `ENV` (all/dev/prod).
- Shared helpers live in `playbooks/lib/common.sh`.
- **Vault-read approval is scoped by run mode:**
  - **Interactive, operator-run gates** that read vault require explicit approval via `ALLOW_VAULT_READS=1` (e.g. `operations/ip_allowlisting`).
  - **Unattended gates run by CI** (`founding/02_preflight`, `founding/06_runner_bootstrap_dev`) are exempt — reading vault to verify presence is their sole purpose and CI cannot prompt for approval.
  - **Destructive teardown gates** (`operations/teardown/*`) do not use `ALLOW_VAULT_READS`; they carry a stronger guard — `ALLOW_<RESOURCE>_TEARDOWN=1` plus typed-environment confirmation.

## When to Add a Gate

Add a gate when:

- A playbook has acceptance criteria that can be verified programmatically
- CI needs to block on a precondition (credential check, host readiness, service health)
- An agent needs to verify state before executing the next playbook step

Not every playbook needs a gate. `founding/01_bootstrap` is human-only with manual verification.
