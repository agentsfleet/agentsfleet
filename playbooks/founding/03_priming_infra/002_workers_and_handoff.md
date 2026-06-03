# Worker Infrastructure + Deployment Handoff

This appendix keeps worker/bootstrap and handoff details separate from `001_playbook.md` so the primary priming file stays within repository line limits.

## Worker Infrastructure Entry Points

- DEV worker canonical playbook: `playbooks/founding/06_runner_bootstrap_dev/001_playbook.md`
- PROD worker canonical playbook: `playbooks/founding/07_runner_bootstrap_prod/001_playbook.md`

## Deployment Handoff Entry Points

- DEV deploy canonical playbook: `playbooks/founding/04_deploy_dev/001_playbook.md`
- PROD deploy canonical playbook: `playbooks/founding/05_deploy_prod/001_playbook.md`

## Notes

- `deploy.sh` remains server-resident and CI-invoked.
- Tailscale remains the private worker SSH/control plane.
- KVM and Firecracker prerequisites still apply to worker nodes.
- Vault contract remains `ZMB_CD_DEV` and `ZMB_CD_PROD` using `op read`.
