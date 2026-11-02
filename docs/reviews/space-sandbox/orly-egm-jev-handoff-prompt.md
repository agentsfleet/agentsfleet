# Improve Orly's problem-to-spec workflow with EGM and Jev

Work in the repository that owns Orly's source and managed skills.
Read its own instructions and inspect its working state before making changes.
Treat this as a separate Orly change; do not patch `agentsfleet`'s generated rules, hooks or gates.

## Problem to solve

The owner, Kishore or Indy, wants a coherent path from discussing a problem to a reviewed, testable implementation spec.
Preserve useful conversation with Indy and recover its accepted decisions across sessions.
Avoid repeated questions, competing design documents and silent expansion of an already active pull request.

The `agentsfleet` SPACE discussion exposed these failures:

- A request for sandbox brainstorming interrupted unfinished model-window compaction without a clear return point.
- The concept brief, concept review, detailed design, implementation spec and delivery readiness were too easy to confuse.
- The agent discussed runtime implementation before presenting the design and its remaining review steps clearly.
- Important scenario decisions lived in conversation until the owner repeatedly requested explanations.
- A later agent needs portable source evidence and accepted decisions rather than paths into a previous agent's temporary directory.

The owner requested a small Firecracker wrapper informed by useful open-source SPACE design and source.
He also required the existing compaction work to finish in pull request 784 and sandbox implementation to follow in a separate stream.
Use `docs/reviews/space-sandbox/` from that merged pull request as a concrete example, not as a universal architecture.

## Inspect existing capabilities first

EGM means Elephant-Goldfish Model.
Read the installed `egm-brainstorm`, `egm-draft`, `egm-review` and resolution skills, including their host integration instructions.
Understand their fresh-context reviews, document outputs and actual owner approval points before designing an adapter.

Read Orly's `skills/orly-spec-new/SKILL.md`, spec template, lifecycle instructions, spec validator and related tests.
Keep the spec as the implementation scope and verification record.
Keep design reasoning in a linked design document, with one home for each decision.

Jev is TypeSafe's System One model.
Inspect the current `orly judge` implementation, command help, `docs/JUDGMENTS.md`, replay storage and tests.
The inspected command help already exposes `orly judge spec <SPEC> --min-score 90` and manifest-based plan, verify, review and document advice.

Check the installed Orly version against the source checkout before assuming those capabilities match.
Determine whether the existing spec grader already evaluates rubric quality before adding another grading path.
Record its actual criteria, score meaning, minimum score and missing-evidence behavior.

## Current source checkpoint

At Orly 0.27 (`700df9c125530a4d2b0d356ee6f0511c536ca35b`), the earlier `judge` command and `src/judgments` implementation are no longer present.
The command-help and transport references above describe the earlier inspection, not a reusable current adapter.
Recheck the selected source before proposing Jev integration; M15_002 only owns the 0.28 cleanup.

## Desired workflow

1. Capture the problem with Indy in plain language, using existing conversation, repository scenarios and prior decisions before asking for facts again.
2. Maintain one visible concept brief with goals, exclusions, accepted choices, unresolved questions and the active work's return point.
3. Use EGM to challenge the concept with fresh reviewers after the owner confirms readiness.
4. Produce a detailed design and an Orly spec with linked mechanisms, exact scope, prerequisites and behavioral proofs.
5. Review both documents through EGM with the companion document and a curated current-code reading list.
6. Use Jev to assess whether requirements, observable outcomes, failure cases and rubric checks support the stated goal.
7. Resolve the findings, obtain the owner decisions that remain and hand implementation to an agent with explicit readiness and dependencies.
8. Continue Orly's existing implementation, verification, review, publication and merge process.

Choose how existing skills hand off; do not create another parallel lifecycle or duplicate their full instruction text.
Explain whether rubric grading belongs in spec authoring, design review or both, using inspected behavior and measured usefulness.
Ask only about a real missing fact or owner decision; do not use a grading score to repeat settled preferences.

## Requirements for the improvement

Preserve clear states: concept reviewed, detailed design reviewed, spec structurally valid, ready to implement, behavior verified and ready to merge.
A passed document review or high Jev score must never mark runtime tests passed or manufacture owner approval.
An unfavorable or uncertain grade must name the unsupported requirement and evidence needed to resolve it.

Require each rubric row to name an observable outcome, the real check, its expected result and where evidence will be recorded.
Attack circular prerequisites, vague expectations, duplicate scoring surfaces and checks that merely mirror the implementation.
Distinguish a spec's quality grade from the later measured grade of its completed acceptance rubric.

Preserve fresh reviewer contexts and verify their actual completed reads and results.
An interrupted, inaccessible or partial review must stay incomplete.
Bind reviews and Jev replays to the exact document and selected source content; changed inputs must invalidate stale advice.

Preserve Orly's existing safety and secret-handling rules.
The inspected CLI defaults to exact offline replay; missing or changed inputs return exit 2.
Its `--refresh` option permits scanned source upload and live provider requests, so preserve that explicit authorization boundary.

Do not make unrequested live model calls, print credentials or treat uploaded-source permission as permission to send arbitrary files.
Use the existing Jev transport and bounded evidence selectors where suitable.
Do not install another model client merely to rename an existing capability.

Record interruptions explicitly: active spec, unfinished checks, authorized detour, return condition and whether the new implementation needs a separate stream.
Recover those facts from durable documents after context loss, and explain the next action to Indy without requiring a transcript reconstruction.

Keep owner decisions separate from reviewer advice and agent defaults.
State what was approved, why a decision is still needed and which concrete result the owner is reviewing.
Avoid approval loops for routine reversible work that the owner has already authorized.

## Deliver and prove

Use Orly's own spec process before implementation.
Prefer changes to its authored skills and templates; propose gate behavior changes explicitly with evidence for their effect.
Do not weaken a failed check, add a baseline or hide a finding to obtain a passing workflow.

Prove the flow with this sandbox example and a small unrelated change.
Cover an active-stream detour, restored context, settled owner decisions, incomplete reviews, stale scores, missing credentials and unsupported evidence.
Use offline fixtures for deterministic checks and request explicit authorization before any live Jev comparison.

Demonstrate that the implementation agent receives one coherent scope and proof mapping.
Show that the owner can identify the current step, remaining decisions and the point at which implementation may begin.
Document the supported handoff, run the owning repository's required checks and open its own pull request.
