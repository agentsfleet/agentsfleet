---
name: orly-burst
description: |
  Run declared burst lanes on a machine rented for the run (AWS Spot or E2E
  Networks) or in a CI job, and capture or prune their warm snapshots. Use on
  "run acceptance on a burst box", "capture a snapshot", or when a Pull
  Request needs acceptance evidence. It costs money: never run it unasked or
  in `orly gate pr`.
---

# orly-burst

This skill owns one `orly burst` run, from choosing the lanes to reporting
the evidence. The lanes, setup, warm-up, secrets and result paths are the
repository's, declared in `.orly/orly.json`. One-time setup, settings and
costs are in orly's `docs/BURST.md`.

## When

- Only when the user asks, or when a Pull Request needs acceptance evidence
  the gates cannot give. Every cloud run rents a machine by the hour.
- Never inside `orly gate pr`, a hook or a retry loop. The gates never run
  `burst.*` commands, and nothing runs them on the gates' behalf.

## Steps

1. **List the lanes.** Run `orly burst`. It prints the declared lanes and exits
   2; pick only lanes it lists.
2. **Pick the provider.** Use `aws-spot` or `e2e` where outbound SSH on port
   22 works, and the repository's burst workflow from an HTTP-only session.
   Inside a CI job, `--provider here` runs the lane on the runner itself.
3. **Warm once.** A repository with no snapshot starts every lane cold from a
   base image. Run `orly burst snapshot --provider <P>` after setup changes,
   and `orly burst snapshot --prune --provider <P>` to keep the newest two.
4. **Run.** Run `orly burst <lane>... --provider <P>`, with `--ref <branch>`
   for a ref other than the current branch. Lanes run in parallel, one machine
   each. `--keep` leaves machines running for inspection, and they keep
   billing.
5. **Report.** Quote every `orly burst: lane=…` summary line and the `total`
   line verbatim, with the evidence directory. Only a numeric `exit=` is
   evidence. `exit=interrupted`, `deadline`, `cancelled`, `unlaunched` or
   `not_run` proves nothing about the code; report that lane as not run.

## Credentials

- Never print a credential, pass one as a command-line argument, or write one
  to a file.
- `burst.secrets` holds `op://` references. orly uses a value already in the
  environment, or reads it with `op read`, on the caller. The value reaches
  the machine only over the SSH session's standard input.
- AWS credentials come from the caller's `aws` configuration or the job's
  cloud login step. E2E credentials are a TIR API token's `E2E_API_KEY`,
  `E2E_AUTH_TOKEN` and `E2E_PROJECT_ID` in the environment; MyAccount keys do
  not work.
- A snapshot never receives a secret: setup and warm run without one.
