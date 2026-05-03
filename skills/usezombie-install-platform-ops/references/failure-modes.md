# Reference — failure modes

Every step of the install plan can fail. The skill stops on the first
failure and surfaces what went wrong so the user can fix it deliberately.
This document is the lookup table the agent loads when an exit happens.

| Step | Mode | Cause | What the skill prints | What the user does |
|---|---|---|---|---|
| 1 — doctor preflight | `auth_token_present: false` | User not logged in | `Run zombiectl auth login first` | `zombiectl auth login`, retry skill |
| 1 — doctor preflight | Workspace not bound | CLI never picked a workspace | Doctor's `workspace` block verbatim | `zombiectl workspace use <name>`, retry |
| 1 — doctor preflight | Vault unreachable | API is down or behind a network split | Doctor's `vault` block verbatim, with the API URL | Check status page, retry once back |
| 3 — repo detection | No `.github/workflows/` | Repo doesn't use GitHub Actions | `GitHub Actions only in v1` | Either set up a GH Actions workflow, or wait for the next-CI-providers milestone |
| 3 — repo detection | Multiple deploy workflows | Ambiguous which workflow is production | List of workflow file names, prompt to pick one | Pick the production-deploy workflow file |
| 5 — webhook secret | `zombiectl credential show github --json` fails | API down or vault unreachable | `zombiectl credential show github` stderr verbatim | Resolve the API/vault issue, retry |
| 6 — credential add | `zombiectl credential add` fails | API down or auth expired | `zombiectl credential add <name>` stderr verbatim | Resolve, retry |
| 6 — credential add | User aborts at masked prompt | User chose to stop | Empty value detected; re-prompt up to 2× then exit | Run skill again with creds in `op` or env to skip prompts |
| 7 — template read | `~/.config/usezombie/samples/platform-ops/` missing | npm postinstall skipped or install corrupted | `Cannot find platform-ops template at ~/.config/usezombie/samples/platform-ops/. Reinstall: npm install -g @usezombie/zombiectl` | `npm install -g @usezombie/zombiectl`, retry |
| 8 — substitution | `.usezombie/platform-ops/` already exists | Re-running on same repo | Prompt overwrite (default `N`) | Choose `Y` to overwrite, or exit and remove the directory manually |
| 9 — install | Response missing `webhook_url` | API contract regression | Captured JSON verbatim, then `install JSON missing webhook_url — file an issue` | File issue with the JSON; retry once a fix ships |
| 9 — install | HTTP 5xx from API | API outage | `zombiectl zombie install` stderr verbatim | Wait for status page, retry |
| 10 — webhook self-test | Receiver returns non-202 | HMAC mismatch, receiver bug, or wrong zombie_id | Receiver's response body verbatim, plus the curl command that was run | Re-run skill (often a transient credential-write race); if persistent, file with the response body |
| 10 — webhook self-test | Network error to api.usezombie.com | DNS, captive portal, firewall | `curl` stderr verbatim | Resolve network, retry |
| 10 — webhook self-test | HMAC mismatch with stored secret | Race between `credential add` and `credential show` cache, or local CSPRNG bug | Computed signature verbatim alongside the receiver's expected | Retry once; if persistent, regenerate via `--force` |
| 12 — smoke steer | Round-trip > 60 seconds | Worker not picking up event | `zombie installed but first response slow — check zombiectl events {id}` | `zombiectl events {id}` to see where it hung |
| 12 — smoke steer | `zombiectl steer` returns error | Zombie status not `active`, or RPC failure | `zombiectl steer` stderr verbatim | Check `zombiectl status {id}`, then retry |
| post-install | npm postinstall logged a warning | FS permission issue, full disk, weird platform | `~/.config/usezombie/samples/` was not populated; user will hit step 7's "missing template" path on next install | Fix the FS issue, re-run `npm install -g @usezombie/zombiectl` |

## What the skill never does on failure

- **Never silently retry.** Each failure surfaces and waits.
- **Never partially install.** If any step after the credential-add
  step fails, the credentials remain in vault but the zombie is not
  installed. Re-running the skill on the same repo picks up the
  vault state (skip-if-exists default) and re-runs the install
  sequence cleanly.
- **Never print a credential value.** The webhook_secret is the only
  generated value the user ever sees, and it appears once in the
  step-11 post-install summary. Never re-display it after.
- **Never advance to the GitHub-paste step on a failed self-test.**
  Step 10's verification is a hard gate on step 11.

## What the user should always check first

Two checks resolve ~80% of install failures without filing an issue:

```bash
zombiectl doctor --json | jq .
zombiectl --version
```

Doctor's output names the failed subsystem. The version output catches
"installed long ago, drifted from the API" cases — `npm install -g
@usezombie/zombiectl@latest` is usually enough.
