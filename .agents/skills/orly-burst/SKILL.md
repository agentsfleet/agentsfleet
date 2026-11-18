---
name: orly-burst
description: |
  Run requested burst lanes on AWS or E2E Networks, and manage their
  snapshots. Use on "run acceptance on a burst box" or
  "capture a snapshot". Paid runs need explicit authorization; missing PR
  evidence does not authorize a run. Never run from a gate or hook.
---

# orly-burst

This skill owns one `orly burst` run, from choosing the lanes to reporting
the evidence. Lanes, setup, secrets, results, region and image are the
repository's `burst` settings in `.orly/orly.json`. Setup and costs:
https://github.com/agentsfleet/orly/blob/main/docs/BURST.md

## When

- Only when the user explicitly authorizes the run. Missing Pull Request
  evidence is a reason to propose lanes, a provider and expected cost for
  approval. It never authorizes a cloud run, snapshot or paid retention.
- Never inside `orly gate pr`, a hook or a retry loop. The gates never run
  `burst.*` commands, and nothing runs them on the gates' behalf.

## Steps

1. **List the lanes.** Run `orly burst`; pick only lanes it lists.
2. **Pick the provider.** Use `aws-spot` or `e2e` where outbound SSH on port
   22 works and the caller stays on one network for the run. From an
   HTTP-only session, start the repository's burst workflow instead.
3. **Warm once.** Capture `orly burst snapshot --provider <P>` after setup
   changes, and prove a new provider with one capture before any lane.
   `orly burst snapshot --prune --provider <P>` keeps the newest one.
4. **Run.** `orly burst <lane>... --provider <P> [--ref <branch>]`. Record
   orly's PID from `$!`, and stop it only with `kill -INT <pid>`, so it
   deletes what it rented.
5. **Report.** Quote every `lane=` line and the `total` line verbatim. Only a
   numeric `exit=` is evidence; any other value means the lane did not run.

   For a numeric failure, read `<evidence>/run.log` and name the failing step
   and test. Name any 🔴 line's resource as left to delete by hand.

## Progress

Before a run, print one line: lanes or snapshot, provider, region, image, ref
and hourly price. Relay each `orly burst:` line as it arrives. After it,
print the result lines and what was deleted.

## Credentials

- Never print a credential, pass one as a command-line argument, or write one
  to a file.
- `burst.secrets` holds `op://` references, read on the caller. A value
  reaches the machine only over the SSH session's standard input, and never
  reaches a snapshot.
- AWS credentials come from the caller's `aws` configuration or the job's
  login step. E2E needs a TIR API token's `E2E_API_KEY`, `E2E_AUTH_TOKEN` and
  `E2E_PROJECT_ID`; MyAccount keys do not work.
