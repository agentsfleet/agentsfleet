# Pilot code review charter

Target: frozen pilot artifacts in the owned `feat/m216-jev-assertion-pilot`
worktree. Functional command-line checks only; no browser, product service,
datastore reset, provider call or external review model.

Rules to prove: reconstruct the frozen source; discriminate seeded faults;
refuse live execution without owner approval; retain twenty unmeasured slots;
check accounting and duplicate reservations with synthetic isolated unit data.
Sources: the active spec's R1–R3 rubric, pilot command help and runner tests.

Entrypoints: `pilot.py check`, `pilot_test.py`, `pilot.py live` with approval
absent, `pilot.py replay` and `pilot.py summarize`. Expected outcomes: 20 correct
passes, 8 assertion failures on deliberately wrong behavior, 12 survivors;
unit checks pass; live refuses before a provider request; replay requests 0;
summary remains IN_PROGRESS and paired review unmeasured.

Owned writes: pilot receipts and disposable proof/test copies. Evidence files
stay under this charter's directory. No approval or live ledger is created.
The unapproved-live command must exit 2 with the missing-owner-approval reason.
Dependencies belong to this worktree; Node runs the incumbent React test stack.

Risks: malformed replies corrupt totals, duplicate launches exceed the budget,
or absent independent reviews are mistaken for measured improvement. Unit
checks cover synthetic malformed/native answers, failed attempts, concurrency,
evidence changes and review contamination without requesting model advice.

Exit condition: current-input required checks pass, or their exact blockage is
reported. Smoke uses the installed five-minute guard; required checks have
finite command deadlines. The results belong in `../../report.md` alongside
the pilot results, with native code review distinct from blinded review benefit.
