# Pull-request and merge-request description dispatch

Read this file before writing or updating a Pull Request (PR) or Merge Request
(MR) body, including an update that only appends session notes. It governs
GitHub and GitLab equally.

## Trigger

This dispatch fires before any forge command or API call that creates or changes
a PR or MR body. This includes `gh pr create`, `gh pr edit --body`,
`glab mr create`, and `glab mr update --description`. A helper, skill, or script
that writes the body fires the same rule.

## Append unit and sequence

Append exactly one complete unit per working session that touches the PR or MR,
after review and verification have finished:

1. `## Session notes <N>`
2. `## Review`
3. `## Make`

Read the complete existing body before composing the unit. Find every heading
that exactly matches `## Session notes <integer>`. Use 1 when none exists;
otherwise use the highest integer plus 1. Re-read the body immediately before
the write and recompute the number if another session appended meanwhile. The
forge body is the source of truth; chat, a local draft, and commit count are not.

Append the unit once, preserving every earlier numbered unit. Never renumber,
rewrite, reorder, or consolidate earlier units. Build the complete unit locally
and make one body update so Review and Make cannot be mistaken for missing.

For GitHub, read with `gh pr view --json body --jq .body` and write with
`gh pr edit --body` or `--body-file`. For GitLab, read with
`glab mr view --output json --jq .description` and write with
`glab mr update --description`. Preserve all body content that precedes the
first numbered unit.

## Session notes <N>

Start with two or three lines of plain prose, never more. Lead with what was
fixed. Include decisions and open deferrals that the next agent needs. Do not
add a preamble or restate the ticket.

Follow with a pictorial explanation using American Standard Code for
Information Interchange (ASCII) diagrams in fenced code blocks. The picture is
the substance when it can show a before/after, topology, proportion, measured
scale, or broken flow more clearly than a sentence. Use the sentence when it is
clearer. Never draw a box that merely repeats its caption.

Every diagram must:

- stay at or below 78 columns, including indentation;
- be inside a fence so both forges render it monospaced;
- label axes and units where they exist;
- use measured values copied from real command output, never illustrative
  values.

These are real examples from a test-hardening PR. Copy their register, never
their numbers:

```text
   BEFORE                              AFTER
   ┌──────────────────────────┐        ┌──────────────────────────┐
   │ 671 tests  ........ PASS │        │ 671 tests  ........ PASS │
   │   incl. all 5 properties │        │ scope property ... FAIL ✓│
   └──────────────────────────┘        └──────────────────────────┘
     a live cross-tenant read            the mutant is caught
     ships green
```

```text
   compiler accepts 16 keys        generator emitted 8
   ┌────────────────────────────────────────────────┐
   │████████████████░░░░░░░░░░░░░░░░│  BEFORE  50%   │
   └────────────────────────────────────────────────┘
```

```text
   0.001ms      0.01      0.1        1         10        100
   ├────────────┼─────────┼──────────┼─────────┼──────────┤
        ▲                  ▲          ▲            ▲
    cache HIT          NEW budget  OLD budget   cache MISS
   0.003-0.005ms        0.1ms        1.0ms      12-21ms
```

## Review

Lead with the review result. Record every finding with P0, P1, or P2 severity,
the file and line, the defect, and its disposition. Use `FIXED` or `DEFERRED`;
a deferral also needs the user acknowledgement required by the lifecycle rule.

```text
P0  oql_property_tests.rs:86  tenant-scope property asserted absence only —
                              passed with the scope clause deleted.  FIXED
P1  query_load_test.rs:183    only assertion was arithmetic, green at any
                              latency.  FIXED
P2  grpc_auth_test.rs:33      constant re-typed instead of imported.  DEFERRED
```

Write `No findings.` when review ran and found none. If review did not run,
write `Review not run — <reason>.` A missing heading never means clean.

## Make

Lead with the verification state. List each command actually run with its exit
status and decisive output line. Use measured counts and baselines from that
run; never replace output with “tests pass.”

```text
make test              exit 0   860 passed; 0 failed   (baseline 846, +14)
cargo clippy --all-targets -- -D warnings   exit 0
make harness-verify    All checks passed!
gitleaks               no leaks found, 54 commits scanned
```

For every command that could not run, name the command and the reason. Label
the evidence source when it is not Continuous Integration (CI), such as
`local — real stack`. Include the review skill and `orly-babysit-prs` command
outcomes here; if either could not run, name why. Never present a local run as
a green pipeline.

## Whole-body rules

- Every factual claim traces to an actual command output line in Make or to a
  command-named measurement in the diagram. State unchecked facts as
  `Unverified: <fact> — <what would verify it>`.
- Lead with the answer in each section. Put reasoning after it and detail last.
- Use no marketing adjectives, throat-clearing opener, or closing paragraph
  that repeats the body.
- Write for the next agent first: branch state, decisions, and open work must
  be findable without reconstructing the session.

## Complete template

````markdown
## Session notes <N>

<Outcome in two or three lines, backed by the evidence below.>

```text
<Useful measured diagram, at most 78 columns.>
```

## Review

<P0/P1/P2 entries with dispositions, No findings., or not-run reason.>

## Make

<Commands paired with exit status and decisive real output lines.>
````
