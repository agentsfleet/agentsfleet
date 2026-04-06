# Playbooks

Canonical layout uses ordered directories so agents can run lexically from `001` upward.

```
playbooks/
├── README.md                          ← this file
├── 001_bootstrap/
│   └── 001_playbook.md
├── 002_preflight/
│   ├── 001_playbook.md
│   └── 001_gate.sh
├── 003_priming_infra/
│   ├── 001_playbook.md
│   └── 002_workers_and_handoff.md
├── 004_deploy_dev/
│   └── 001_playbook.md
├── 005_deploy_prod/
│   └── 001_playbook.md
├── 006_worker_bootstrap_dev/
│   ├── 001_playbook.md
│   └── 001_gate.sh
├── 007_worker_bootstrap_prod/
│   └── 001_playbook.md
├── 008_credential_rotation_dev/
│   ├── 001_playbook.md
│   └── 001_gate.sh
├── 009_grafana_observability/
│   ├── 001_playbook.md
│   └── 001_gate.sh
├── 010_data_plane_ip_allowlisting/
│   ├── 001_playbook.md
│   ├── 001_gate.sh
│   ├── 001_gate_section_1.sh
│   └── 002_gate_section_2.sh
├── lib/
│   └── common.sh
└── gates/
    ├── check-credentials.sh           ← credentials gate entrypoint
    ├── m2_001/
    │   ├── run.sh                     ← runner (dispatches sections)
    │   ├── section-1-preflight.sh     ← checks op CLI + auth
    │   └── section-2-procurement-readiness.sh  ← checks all vault items
    ├── m4_001/
    │   ├── run.sh
    │   ├── section-1-ssh-access.sh
    │   ├── section-2-host-readiness.sh
    │   └── section-3-deploy-readiness.sh
    ├── m7_002/
    │   ├── run.sh
    │   ├── section-1-vault-sync.sh
    │   └── section-2-service-health.sh
    └── m28_001/
        ├── run.sh
        ├── section-1-credentials.sh
        ├── section-2-prometheus.sh
        └── section-3-dashboard.sh
```

## Playbooks vs Gates

**Playbooks** (`playbooks/NNN_name/001_playbook.md`) are human-readable runbooks. They describe:

- Who does what (human vs agent)
- Step-by-step procedures with context and rationale
- Acceptance criteria per step
- Dependencies and prerequisites

Playbooks are documentation. They are NOT executable.

**Gates** (`playbooks/gates/m{n}_{nnn}/*.sh`) are machine-executable verification scripts. They:

- Validate that a playbook's acceptance criteria are met
- Run in CI as pipeline prerequisites (e.g. `deploy-dev.yml` runs `check-credentials.sh` as job 0)
- Run locally by agents to verify state before proceeding
- Exit non-zero on any failure — fail loud, fail all items (not just the first)

Gates are executable. They are NOT documentation.

## Gate Script Convention

Each gate should be runnable from an ordered playbook directory (`playbooks/NNN_name/001_gate.sh`).

- `001_gate.sh` — top-level runner, optionally dispatches to section scripts.
- `001_gate_section_*.sh` — one script per section when needed.
- All scripts are `set -euo pipefail`, print per check, exit 1 if any check fails.
- Environment: `VAULT_DEV`, `VAULT_PROD`, `ENV` (all/dev/prod).
- Shared helpers live in `playbooks/lib/common.sh`.
- If a gate reads from vault, it must require explicit approval via `ALLOW_VAULT_READS=1`.

## When to Add a Gate

Add a gate when:

- A playbook has acceptance criteria that can be verified programmatically
- CI needs to block on a precondition (credential check, host readiness, service health)
- An agent needs to verify state before executing the next playbook step

Not every playbook needs a gate. M1_001 (Bootstrap) is human-only with manual verification.

New references should always use the canonical ordered paths (`playbooks/NNN_name/...`).
