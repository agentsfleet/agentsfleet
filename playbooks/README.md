# Playbooks

Canonical layout uses ordered directories so agents can run lexically from `001` upward.

```
playbooks/
├── README.md                          ← this file
├── 001_bootstrap/
│   └── 001_playbook.md
├── 002_preflight/
│   ├── 001_playbook.md
│   ├── 00_gate.sh                    ← dispatcher (globs 01_*, 02_*, etc.)
│   ├── 01_tools_and_auth.sh
│   └── 02_credentials.sh
├── 003_priming_infra/
│   ├── 001_playbook.md
│   └── 002_workers_and_handoff.md
├── 004_deploy_dev/
│   └── 001_playbook.md
├── 005_deploy_prod/
│   └── 001_playbook.md
├── 006_worker_bootstrap_dev/
│   ├── 001_playbook.md
│   ├── 00_gate.sh
│   ├── 01_ssh_access.sh
│   ├── 02_host_readiness.sh
│   └── 03_deploy_readiness.sh
├── 007_worker_bootstrap_prod/
│   └── 001_playbook.md
├── 008_credential_rotation_dev/
│   ├── 001_playbook.md
│   ├── 00_gate.sh
│   ├── 01_vault_sync.sh
│   └── 02_service_health.sh
├── 009_grafana_observability/
│   ├── 001_playbook.md
│   ├── 002_grafana_setup.md
│   ├── 00_gate.sh
│   ├── 01_credentials.sh
│   ├── 02_prometheus.sh
│   └── 03_dashboard.sh
├── 010_data_plane_ip_allowlisting/
│   ├── 001_playbook.md
│   ├── 00_gate.sh
│   ├── 01_egress_inventory.sh
│   └── 02_provider_targets.sh
└── lib/
    └── common.sh
```

## Playbooks vs Gates

**Playbooks** (`playbooks/NNN_name/001_playbook.md`) are human-readable runbooks. They describe:

- Who does what (human vs agent)
- Step-by-step procedures with context and rationale
- Acceptance criteria per step
- Dependencies and prerequisites

Playbooks are documentation. They are NOT executable.

**Gates** (`playbooks/NNN_name/00_gate.sh` + numbered sections) are machine-executable verification scripts. They:

- Validate that a playbook's acceptance criteria are met
- Run in CI as pipeline prerequisites (e.g. `deploy-dev.yml` runs `002_preflight/00_gate.sh` as job 0)
- Run locally by agents to verify state before proceeding
- Exit non-zero on any failure — fail loud, fail all items (not just the first)

Gates are executable. They are NOT documentation.

## Gate Script Convention

Each gate lives inside its ordered playbook directory.

- `00_gate.sh` — dispatcher. Globs `01_*.sh`, `02_*.sh`, etc. and runs them in order.
- `01_name.sh`, `02_name.sh` — numbered section scripts. Two-digit prefix, descriptive snake_case name.
- All scripts are `set -euo pipefail`, print per check, exit 1 if any check fails.
- Environment: `VAULT_DEV`, `VAULT_PROD`, `ENV` (all/dev/prod).
- Shared helpers live in `playbooks/lib/common.sh`.
- If a gate reads from vault, it must require explicit approval via `ALLOW_VAULT_READS=1`.

## When to Add a Gate

Add a gate when:

- A playbook has acceptance criteria that can be verified programmatically
- CI needs to block on a precondition (credential check, host readiness, service health)
- An agent needs to verify state before executing the next playbook step

Not every playbook needs a gate. 001_bootstrap is human-only with manual verification.
