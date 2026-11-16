---
name: orly-burst
description: |
  Run the full acceptance suite (make acceptance) on a throwaway machine and
  bring the evidence back. Use on "run acceptance on burst", "prove acceptance
  on a clean box", or when a pull request needs a real acceptance run.
---

# Burst acceptance

Until orly 0.40 ships `orly burst`, the only burst machine is GitHub's: the
`acceptance-on-demand.yml` workflow runs `make acceptance` on a fresh runner.
It needs only `gh`, so it works from a Mac and from cloud agent sessions.

```bash
# Before the workflow is on main: label the pull request; every push reruns it.
gh pr edit <number> --add-label acceptance
# Once it is on main: dispatch it for any branch.
gh workflow run acceptance-on-demand.yml -f ref=<branch> -f suite=e2e
gh run watch <run-id> --exit-status
gh run download <run-id> --dir "$TMPDIR/acceptance/<run-id>"
```

Rules:

- Run it only when asked or when a pull request needs acceptance evidence;
  it never belongs in `orly gate pr`.
- The workflow loads its secrets from 1Password with the repository's
  service-account token. Never print, pass or store a credential.
- Report the run's URL and conclusion, then the failing test titles from the
  downloaded evidence. A cancelled or timed-out run proves nothing; say so.
