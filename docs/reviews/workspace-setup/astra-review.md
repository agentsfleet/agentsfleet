# Independent implementation-readiness review

Reviewer: `gpt-6-astra`, high reasoning effort, independent context.
Worktree: `/Users/kishore/.codex/worktrees/fd5a/agentsfleet`.
Branch: `feat/website-completion`.
Comparison revision: `ad3ed8f92c739b59df71863dbeec81d621dd7d0a`.

One read-only review covered the working tree, including untracked documents. No files, fixtures, branches, or worktrees were changed by the reviewer. No tests were run. References below identify the documents at review time; architectural guidance is subsequently integrated into the common topic pages.

## Verdict

**APPROVE WITH REQUIRED CHANGES.** The ten earlier findings have concrete implementation requirements and mapped proofs. Two narrow inconsistencies require repair before the affected paths. Neither requires an owner decision. These are specification dispositions, not demonstrated runtime behavior.

## Remaining findings

### Accounting-only settlement after content expiry — P2, confidence 8/10

References: `chat_authoring_api_resources.md:197,219`; `chat_authoring_runner.md:84,92`.

Counterexample: an answer-bearing report commits but its acknowledgement is lost. The runner stays offline past the spool deadline. Startup deletes the answer and preserves accounting. Resending the report identifier without the answer changes the original body; retaining the answer violates expiry. The wire does not define which content-free retry the daemon accepts.

Required repair: define an accounting-only variant on the existing `/reports` route. Bind replay to immutable validated accounting fields independently of private-content submission. Specify original-committed, original-never-received, and deleted-draft behavior. A supplied digest cannot authorize changed usage or restore text. Update the runner guidance and Dimension 3.4.

Required negative proof: lose the original acknowledgement, expire and remove every private copy, then settle/replay from retained accounting only. Repeat when the original request never arrived. Acknowledge terminal accounting without duplicate charges, changed usage, or reconstructed text.

### Initial-run diagrams omit durable Pending — P2, confidence 10/10

References: `chat_authoring_end_to_end.md:247,250`; `docs/designs/workspace-chat-options.md:117`; canonical `chat_authoring_api_resources.md:105`; correct creation description `chat_authoring_creation.md:37,45`.

Counterexample: activation and durable admission commit, then Dragonfly append fails. The canonical result is an active fleet with `start.state:"pending"`. A two-state Accepted/Blocked interface either claims acceptance too early or labels recoverable delivery blocked.

Required repair: show admission recorded with activation, followed by append/receipt completion Pending → Accepted, retaining the same initial event identity. Dimensions 4.3 and 6.2 must prove Pending after failed append or uncertain receipt and Accepted only after recovery, including reload.

## Previous findings

| Finding | Specification disposition and review-time evidence |
|---|---|
| Scoped model authorization | Resolved requirements: daemon keys, bearer plus bound capability, committed dispatch identity, disabled retries, revocation boundary and unknown-usage fencing. API resources:204–217. |
| Capacity during renewal | Resolved requirement: common runner guard for claims and both renewals; original expiry cannot revive. Runner:23–25. |
| Candidate identity and replay | Resolved: current candidate in lease, immutable identity/revision per change, exact acknowledgement replay returns original version. API resources:184–200. |
| Admissions without receipts | Recovery and logical-identity deduplication resolved; diagrams need the second repair. API resources:155. |
| Model replacement | Resolved: idle guarded replacement, owner eligibility, durable selection, earlier pinned turns. API resources:125,141. |
| Durable failure actions | Resolved: bounded reasons/codes/actions, atomic persistence, authoritative unknown-usage precedence. API resources:89–101,202. |
| Definition validation | Resolved: valid slug/version, supported metadata and round-trip/rejection proofs. Definition:35–64,84. |
| Spool retention | Honest physical/decryptability limits and supervisor cleanup specified; accounting replay needs the first repair. Runner:79–94. |
| Explicit Library publication | Resolved in specification/diagram; source correction remains pending. M227_002:84,117; website-job-entrance:43–47. |
| Matching-build acceptance | Resolved procedure: four observed identities, startup evidence, preflight before mutation, mismatch proofs. Public changes:54–60; M227_001:252. |

## Rust organization and evidence limits

One production authoring crate, HTTP adaptation in existing API crates, execution in existing runner crates, focused domain modules/tests. Follow workspace dependencies/lints, small documented roots, local runtime/admission examples and applicable Rust guidance (`M-MODULE-DOCS`, `M-FROM-ERROR`, `M-SIMPLE-ABSTRACTIONS`). Actual dependency direction, file lengths and errors require implementation verification. The supplied Exonum checkout is absent.

Inspected implementation still uses raw runtime model access (`afd_fleet/src/lease/answer.rs:79–81`), lacks original-expiry renewal rejection (`afd_fleet/src/lease/sql/renew.rs:78–97`), and excludes receipt-less queued rows (`afd_events/src/history/statement/queued.rs:13`). Keep proofs pending until run. Preserve L1–L21 and verify the independent schedule prerequisite before schedule acceptance.
