# Blinded review measurement checks

The command-line pilot must summarize original independent decisions and reject a broken review seal.
Sources: `pilot_measure.py:210–256`, `README.md` review protocol, and the active Milestone 216 (M216) spec, Dimension 3.3.

Scope: local summary, review-seal refusals, and the existing pilot runner checks.
Writes stay in the pilot summary and this review receipt directory.
No provider requests, product writes, shared datastores, or dependency installation are needed.

Inputs are two frozen agent records, their original timing events, unchanged cases, and retained live advice.
Each reviewer received the same selected source evidence, with separate shuffled orders.
The assisted packet adds only the original retained advice.

Commands:

- `python3 pilots/jev-assertions/pilot.py summarize`: paired status `measured`; two complete sessions; original attempts and usage unchanged.
- `python3 pilots/jev-assertions/pilot_test.py PilotTests.test_review_contamination_seal PilotTests.test_review_findings_are_separate_from_classifications`: both checks pass; changed seal refuses comparison.
- `python3 pilots/jev-assertions/pilot_test.py`: 25 passing checks, including refusals and failed attempts; no provider requests.

The smoke window is five minutes with at most twelve probes.
Required Section proofs remain distinct from full repository checks due before a Pull Request.
Stop after the named checks have complete safe receipts, or record the affected check as incomplete.

Review times are measured agent elapsed seconds, including reasoning and tool latency.
Different reviewers, ordering, and small timer-boundary differences prevent a causal time-saving claim.
Human review time remains unmeasured.
