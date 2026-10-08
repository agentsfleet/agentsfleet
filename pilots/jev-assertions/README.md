# Jev assertion pilot

Run a frozen contributor experiment on `agentsfleet` tests using the existing
`orly` question `verify.assertion`. Its five native answers are `exact`, `weak`,
`wrong_target`, `missing`, and `insufficient`. Advice stays advisory.

The [active spec](../../docs/v2/active/M216_001_P2_DOCS_INFRA_JEV_ASSERTION_REVIEW_PILOT.md)
owns scope and lifecycle. [report.md](report.md) records results, evidence and
limitations. Expected classifications and justifications live in
[cases.json](cases.json); never include that file in a model request.

## Reproduce offline

Use this implementation worktree and the source revision recorded in the ledger.
Install only its own frozen dependency graph, using `mise` (the tool version
manager), and then execute the checks:

```sh
mise exec -- bun install --frozen-lockfile
(cd ui/packages/design-system && mise exec -- bun install --frozen-lockfile)
python3 pilots/jev-assertions/pilot.py check
python3 pilots/jev-assertions/pilot_test.py
orly judge verify --input pilots/jev-assertions/manifests/a.json --json
orly judge verify --input pilots/jev-assertions/manifests/b.json --json
python3 pilots/jev-assertions/pilot.py replay
python3 pilots/jev-assertions/pilot.py summarize
```

The runner checks the [frozen byte inventory](freeze.json) before execution.
`check` reconstructs source from the recorded git revision, applies the
listed import and named-constant adaptations, and executes correct and deliberately faulty temporary copies.
The checked-in implementation is correct for exact/insufficient cases and
faulty for weak/wrong_target/missing cases. Proof copies are cleaned after each
run. Use Node for the copied Vitest/jsdom stack; running its workers under Bun
failed environment setup in this preparation. The recorded runtime versions
are in the report.

Expected proof totals are 20 correct passes and, on incorrect behavior, 8
assertion failures plus 12 passes. Four failures come from visible exact tests;
four come from strong local assertion helpers omitted from insufficient model
inputs. An insufficient case concerns missing evidence, not a weak hidden test.

An uncached offline `orly` command exits 2 with requests 0. The runner retains
that native exit in its receipt and exits 0 after recording it; this means
receipt collection succeeded, not that advice became available. Never invent
cache entries or retry with refresh to resolve this offline result.

## Review protocol

The [frozen author inspection](reviews/unaided-author.json) predates all advice
commands. The fixture author knows the expected labels; its untimed decisions
cannot establish independent reviewer benefit.

If independent reviewers become available, use two separate sessions with
different reviewers who have not seen the ledger, seeded mutations, author
decisions or this report's answer table. Pair observations by the same neutral
case identifiers. Give both reviewers the requirement and exactly the manifest
evidence, including identical deliberate omissions. Require all five classes,
a finding decision, evidence references, and measured active review seconds.
Count missing context separately from an inadequate assertion.

1. Give the unaided reviewer a shuffled order of all twenty items without
   Jev advice. Freeze `reviews/unaided.json` before obtaining or revealing
   advice. Give the assisted reviewer a separately shuffled order, the same
   source packet, and the retained advice, including failures and withholding.
2. Keep reviewers separated: no shared conversation or same-person rereading.
   Blind both to expected classifications and control origin; the assisted
   reviewer necessarily knows advice is present. Preserve their initial
   decisions without corrections after answers are revealed.
3. Fill [session-template.json](reviews/session-template.json) into each actual
   session record. Add `independent: true`, `blinded: true` (to answer labels),
   and timezone-aware `frozen_at` to the unaided record. The assisted record
   adds `advice_first_seen_at`, `unaided_sha256`, and `advice_sources` mapping
   `a` and `b` to the complete live receipt hashes. These are attestations,
   supported by session records; timestamps alone cannot prove blinding.
4. Summarize case-paired changes in confirmed findings, misses, false alarms
   and active review time. Different reviewers introduce skill differences;
   four correlated behavior families limit generalization. Do not claim a
   controlled causal effect from one pair of sessions.

If these sessions are unavailable, leave their files absent and report the
comparison **unmeasured**. Model-label agreement and author inspection remain
separate observations. They cannot substitute for a paired review.

## One approval, bounded live execution

Complete offline preparation, commit the frozen inputs, and obtain Kishore's
actual approval of the exact upload inventory and budget. Before approval,
`approval.json` is absent. No refresh is permitted.

The proposed limit is **20 provider requests total**, failed requests included,
with **United States dollars (USD) 0.06** as the spending ceiling. At the
[documented Jev 1.13.0 input price](https://docs.typesafe.ai/models) of USD 0.042
per million input tokens and free output, reserving 65,536 input tokens for
each request yields USD 0.05505024 for twenty requests. That uses the model's
documented 64k context limit conservatively; it is not a predicted invoice.
Recheck that price and limit before live execution. No provider account dollar
cap has been verified.

The runner requires `approval.json` to contain the actual owner quote and these
values: `approved_by: "Kishore"`, `model: "jev-1.13.0"`,
`provider_requests_max: 20`, `ceiling_usd: "0.06"`,
`input_usd_per_million: "0.042"`, `output_usd_per_million: "0"`,
`max_input_tokens_per_request: 65536`, `frozen_digest`, `runner_digest`,
`pricing_checked_url: "https://docs.typesafe.ai/models"`, and
`pricing_checked_at` with an explicit timezone, confirmed within 24 hours.
Never populate `owner_quote` from a suggested answer or a test stub.

After approval, use the runner:

```sh
python3 pilots/jev-assertions/pilot.py live
python3 pilots/jev-assertions/pilot.py replay
python3 pilots/jev-assertions/pilot.py summarize
```

It executes exactly these commands, once per batch:

```sh
orly judge verify --input pilots/jev-assertions/manifests/a.json --refresh --json
orly judge verify --input pilots/jev-assertions/manifests/b.json --refresh --json
```

Before launching, it locks and persists a full ten-request reservation. Each
batch can be reserved once. A timeout, scanner rejection, failed request,
interrupted process or crash consumes the reservation; there are no retries
or extra planning-model calls. Unknown request counts retain the entire
reservation. Existing live receipts cannot be replaced by another launch.
Keep the ledger and worktree; direct refresh commands or deleting local records
would bypass this local enforcement and violate the approved procedure.

The installed `orly` pin checks the entire batch before upload, scans selected
source, and makes at most one sequential request per item. Approval is bound to
both frozen evidence and the runner's bytes. Native answers and usage remain in
live receipts; unavailable usage is unknown, never assigned zero cost. Private
`.orly/judgments` replay files remain outside committed artifacts.

## Interpret measurements

`summary.json` keeps all twenty case slots and separates native classification
from confidence withholding. Strength is the minimum of answer confidence and
the chosen class's probability; the existing threshold is 0.8. Native
`insufficient` requires inspection even at strength 1, independently of low
strength. The pilot does not change either rule.

Confirmed findings are native weak/wrong_target/missing answers on one of the
twelve seeded inadequate tests. Wrong subclasses can confirm inadequacy while
failing exact classification agreement. False alarms are such answers on exact
or insufficient inputs. Native misses and unavailable weaknesses are separate;
recall uses all twelve known inadequate cases, so failures remain in its
denominator. Report confident concerns separately from native findings.
For paired reviewers, use their explicit `finding` decision for confirmed
findings, misses and false alarms; report classification agreement separately.

Split source-derived exact controls, seeded controls, and newly discovered
production defects. This pilot has no confirmed production defect: the copied
incorrect behavior and weakened assertions are deliberate controls. Provider
elapsed time is not reviewer time. Sum only reported token usage and calculated
known cost; retain unknown usage and the reservation upper bound alongside it.
