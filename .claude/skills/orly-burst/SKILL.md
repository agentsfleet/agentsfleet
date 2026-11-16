---
name: orly-burst
description: |
  Run the full local acceptance suite (make acceptance-codex) on a throwaway
  full VM and bring the evidence back: AWS Spot first, GitHub Actions as the
  free baseline. Use on "run acceptance on burst", "acceptance on AWS", "prove
  acceptance on a clean box", or when local acceptance is blocked by the
  laptop (network drops, keychain, OrbStack) and a pull request needs a real run.
---

# Burst acceptance

One command, three outcomes: the suite's exit code, a summary line, and the
evidence under `$TMPDIR/acceptance-burst/<run>/`.

```bash
scripts/acceptance-burst.sh --provider aws-spot --suite e2e [--ref <branch>] [--keep]
scripts/acceptance-burst.sh --provider gh       --suite e2e [--ref <branch>]
```

Pick the provider:

- `aws-spot` (default): EC2 Spot, Debian 13 amd64, about $0.09 a 20-minute run.
  Needs the one-time setup in `docs/development.md` (Burst acceptance): the
  `ACCEPTANCE_BURST_BUCKET`, an instance profile, and the 1Password token in
  SSM Parameter Store.

  AWS credentials come from the caller's environment:
  `aws sso login` on a Mac, an AWS SigV4 network secret in a Claude cloud
  session. Only HTTPS to AWS is used, never SSH.
- `gh`: the `acceptance-on-demand.yml` workflow on GitHub's runners, free
  minutes. A dispatch works only once the workflow is on `main`; before that,
  label the pull request `acceptance` instead of running this provider.

Rules:

- Run it only when asked or when a pull request needs acceptance evidence; it
  spends money and never belongs in `orly gate pr`.
- Never print, pass on the command line, or write a credential. The instance
  reads the 1Password service-account token from SSM itself.
- Report the summary line verbatim (provider, minutes, Spot price, exit,
  evidence path), then the failing test titles from the evidence. A run that
  stopped early (`exit=interrupted` or `exit=deadline`) proves nothing; say so.
- `--keep` leaves the instance running for debugging; end it afterwards
  with `aws ec2 terminate-instances` on the id the run printed.
