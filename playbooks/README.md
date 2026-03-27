# Playbooks

```
playbooks/
├── README.md                          ← this file
├── M1_001_BOOTSTRAP.md                ← playbook (human-readable)
├── M2_001_PREFLIGHT.md
├── M2_002_PRIMING_INFRA.md
├── M3_001_DEPLOY_DEV.md
├── M3_002_DEPLOY_PROD.md
├── M4_001_WORKER_BOOTSTRAP_DEV.md
├── M7_002_CREDENTIAL_ROTATION_DEV.md
└── gates/
    ├── check-credentials.sh           ← legacy shim → m2_001/run.sh
    ├── m2_001/
    │   ├── run.sh                     ← runner (dispatches sections)
    │   ├── section-1-preflight.sh     ← checks op CLI + auth
    │   └── section-2-procurement-readiness.sh  ← checks all vault items
    ├── m4_001/
    │   ├── run.sh
    │   ├── section-1-ssh-access.sh
    │   ├── section-2-host-readiness.sh
    │   └── section-3-deploy-readiness.sh
    └── m7_002/
        ├── run.sh
        ├── section-1-vault-sync.sh
        └── section-2-service-health.sh
```

## Playbooks vs Gates

**Playbooks** (`playbooks/M{N}_{NNN}_*.md`) are human-readable runbooks. They describe:

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

Each gate follows the m2_001 pattern:

- `run.sh` — top-level runner, dispatches to section scripts. Accepts `SECTIONS=1,2,3` env var.
- `section-N-*.sh` — one script per playbook section that has verifiable criteria.
- All scripts are `set -euo pipefail`, print per check, exit 1 if any check fails.
- Environment: `VAULT_DEV`, `VAULT_PROD`, `ENV` (all/dev/prod).

## When to Add a Gate

Add a gate when:

- A playbook has acceptance criteria that can be verified programmatically
- CI needs to block on a precondition (credential check, host readiness, service health)
- An agent needs to verify state before executing the next playbook step

Not every playbook needs a gate. M1_001 (Bootstrap) is human-only with manual verification.
