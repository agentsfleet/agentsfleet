---
type: reference
audience: operator
verified: 2026-11-18
product_version: 0.77.0
executable: false
---

# agentsfleet playbooks

| Goal | Start |
| --- | --- |
| Install on infrastructure you operate | [SELF_HOST.md](./SELF_HOST.md) |
| Prime a migrated self-hosted database | [PRIME.md](./PRIME.md) |
| Build or release agentsfleet.net | [Hosted operations](./hosted/README.md) |
| Recover an installation or enable a capability | [Operations index](./operations/README.md) |

## Chronological setup

Follow the [hosted installation sequence](./hosted/README.md#chronological-setup) for agentsfleet.net.

## Prime a clean database

Use [PRIME.md](./PRIME.md) for a self-hosted database. The [hosted priming sequence](./hosted/README.md#prime-a-clean-database) uses the managed environments and vaults.

## On-demand operations

See the [operations index](./operations/README.md).

## Script shape

- Each playbook directory holds `001_playbook.md`.
- `00_gate.sh` is a thin entry point with an explicit command list.
- `check`, `apply` and `verify` are the standard actions.
- `dev` or `prod` selects one environment; a write never defaults to both.
- Provider code lives under `providers/<provider>/`.
- Shared libraries hold only code with more than one caller.
- `*_test.sh` files are local regression tests.

After any edit, run the local integrity gate:

```bash
make check-playbooks
```

It runs vault-gate parity, path-reference checks, ShellCheck and every shell regression test.
It exits 0 when all pass. A failure prints `✗ <N> regression test file(s) failed`.
