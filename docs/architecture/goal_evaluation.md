# Configurable decision models for intent and goal evaluation

## Status and sources

This page owns the proposed decision-provider boundary for M236 [Durable Tasks](durable_tasks.md).
Indy selected Jev initially and then requested a configurable decision model per runner.
Implementation is approved; live provider proofs and runtime acceptance remain pending.

The initial provider is TypeSafe `jev-latest`; it is a configuration choice rather than a task-runtime dependency.
Provider documentation and crate metadata were checked on November 3, 2026.
Recheck endpoint capabilities, rates and resolved model identity before activation.

## One trait inside the existing provider layer

Keep decision networking in `afr_providers`, alongside existing provider connections and guarded transport.
The agent owns when to ask and how to apply accepted answers; it does not receive provider keys.

Reuse guarded HTTP construction, final request-header validation, native vault resolution, usage reporting and the current lease budget.
Decision responses use a 64-KiB bound; coding inference retains its existing SDK wire limit.

```text
Existing fleet admission -> existing lease -> existing agent loop
                                             |
                              DecisionEvaluator::evaluate
                                             |
                                    DecisionClient
                              one guarded HTTP transport
                              four small native wire adapters
                                  /      |      |      \
                                Jev   OpenAI  Clef  Perplexity
                                             |
                                  typed DecisionResult
                                             |
                          native ownership/evidence/budget checks
                                             |
                                same lease/report/settlement
```

One object-safe `DecisionEvaluator` trait exposes one bounded asynchronous `evaluate` call accepting only the validated decision request.
`DecisionClient` owns its frozen profile and credential; agent-facing calls cannot select or receive provider keys.
Use existing boxed-future conventions; no plugin registry, provider discovery service or second agent loop is needed.

`DecisionRequest` carries purpose, bounded state and reviewed questions; trusted generation/evidence bindings remain outside model-controlled answers.

`DecisionResult` carries typed answers, the reported model and physical usage.
The native reservation binds these results to the frozen provider/profile revision and assessed evidence.

`DecisionCapabilities` declares supported question types, distributions and refusal behavior.
Unsupported capabilities refuse activation instead of inventing probabilities or silently dropping questions.

One `DecisionClient` implements the trait with four small native API adapters.
Share question/answer representations only where provider fixtures prove compatibility; translate envelopes, authentication, usage and refusal semantics explicitly.
OpenAI's `predicate` maps to internal yes-probability semantics; its question/answer arrays differ from TypeSafe's keyed objects.

Existing Rig remains the coding-provider dependency; the decision client reuses guarded HTTP and native accounting directly.

## Configuration per runner

Nullable `decision_model_id` in the existing runner assignment selects reviewed decision metadata from the current model catalogue.
Runner enrollment, edit and heartbeat carry the catalogue identity; lease issue freezes `DecisionAssignment` in its payload and `runner.leases.decision_snapshot`.
Unconfigured hosts skip the catalogue read.

Native reservation, one-time credential issuance and receipt accounting are written in `afd_fleet::task::decision`; runtime integration and acceptance remain pending.

Decision-only entries carry their own price schedule and no fabricated coding rates.
The current model selects an immutable reviewed revision; enabling or disabling it preserves that definition and prior call accounting.
Catalogue reads resolve definitions and prices through one consistent database snapshot.

`model.decision_prices` adds decision quotes without changing the coding-price table or its conflict key.
Both sources share catalogue ownership, price values, validation and billing; their helpers select fixed statements.
`billing.usage_lines` keeps opaque quote identifiers and recorded charges, without a foreign key to either price source.

The curated coding-model importer changes coding schedules only.
It retains decision definitions, decision prices and their model identities when a coding entry is removed.

Pruning a retained model's coding entry clears its coding inference settings, leaving its decision configuration editable.
Decision-only rows do not appear as repeated coding-import deletions.

Upgrade catalogue writers and the coding importer before creating or enabling decision profiles.
The additive price table preserves ordinary coding traffic while older daemons finish serving.

Store thresholds as integer basis points; native evaluation compares them with returned probabilities without rounding away the approved boundaries.
Reuse the native HTTPS endpoint proof for coding and decision settings; guarded transport still enforces DNS at dispatch.

TypeSafe's [model-list API](https://docs.typesafe.ai/api/list-models.md) returns names, descriptions and release dates without context limits.
Its [official limits](https://docs.typesafe.ai/models.md) document per-request and per-question context; decision metadata may cite that provider source.
Coding entries retain the existing models-API source requirement.

The profile binds provider, model, protocol, allowed endpoint, vault reference, capability set, rates, question-format revision and evaluated model versions.
Credentials remain separate vault references; a profile cannot borrow another workspace's credential through a model identifier.
Runner assignment is an operator choice; fleet chat and repository content cannot change it.

```text
Runner A assignment: decision_model_id -> Jev profile
Runner B assignment: decision_model_id -> Perplexity profile
Runner C assignment: decision_model_id -> Cloudflare Clef profile
Runner D assignment: decision_model_id -> OpenAI Decisions profile

operator PATCH existing runner -> native assignment -> heartbeat
                                                 -> lease snapshot -> evaluate
```

Apply assignment edits to later leases; an active call retains its recorded profile and physical call identity.
After the slot lock, a fresh database clock rejects claims that expired during profile lookup or lock waits.

Revocation still prevents new dispatch and native acceptance while preserving measured usage.
Replacement runners may use another reviewed profile without changing the DT objective, authority, history or cumulative limits.

Decision reuse requires matching profile revision, questions, assessed state, goal revision and evidence binding.
A profile change invalidates cached assessments; it does not repeat already accepted human input or confirmed effects.
An unset or unsupported profile leaves DT admission unavailable with a named prerequisite; ordinary chat retains its current path.

## Provider APIs and Rust client choices

| Provider | Verified API/capability | Proposed adapter |
|---|---|---|
| [TypeSafe Jev](https://docs.typesafe.ai/api/system-one.md) | `POST https://api.typesafe.ai/v1/systemone`; Noul, Choice and Score. | `DecisionClient`, TypeSafe wire. Initial model `jev-latest`. |
| [Perplexity Decisions](https://docs.perplexity.ai/docs/decisions/quickstart.md) | `POST https://api.perplexity.ai/v1/decisions`; typed probabilities. Use an evaluated versioned identifier such as `pplx-decider-v1.2`; the response echoes the requested model. | `DecisionClient`, Perplexity wire and usage rules. Flash bills state separately per question. |
| [Cloudflare Clef](https://developers.cloudflare.com/workers-ai/models/clef/) | Workers AI account-scoped `/ai/run/@cf/cloudflare/clef`; native Noul/Choice/Score distributions. | `DecisionClient`, Workers AI envelope. Preflight context bounds because the provider documents text truncation. |
| [OpenAI Decisions](https://developers.openai.com/api/docs/guides/decisions) | `POST https://api.openai.com/v1/decisions`; native predicate, choice and score probabilities. The inspected guide supports `gpt-6-luna`. | Native OpenAI wire in `DecisionClient`; map input, named question arrays, answer arrays, usage and refusals explicitly. |

TypeSafe's [official software development kit (SDK) index](https://docs.typesafe.ai/sdk.md) lists Python and JavaScript/TypeScript, plus direct HTTP.
Rust crates exist: [typesafe-client](https://crates.io/crates/typesafe-client), [typesafe-ai-sdk](https://crates.io/crates/typesafe-ai-sdk) and [jev](https://crates.io/crates/jev).
These are community clients; publication and matching request types do not establish native transport or accounting compatibility.

Inspected metadata shows `typesafe-client` 0.2.0 uses reqwest 0.13; `typesafe-ai-sdk` 0.6.0 uses reqwest 0.13.5.
The inspected `typesafe-client` source accepts an injected client and can disable retries, making it a credible candidate rather than inherently incompatible.

Its 0.2.0 `client/mod.rs:227` collects `response.bytes()` before decoding without this design's 64-KiB streaming cap.
Keep the shared bounded reader unless adopting that client also proves bounded response reads and redacted error bodies.
Compatibility still requires injected guarded transport, disabled hidden retries, redacted errors and exact physical usage.

The `jev` 0.1.2 crate enables reqwest's blocking feature; do not add a blocking path to the runner loop.

The workspace pins Rig 0.46.0 and reqwest 0.13.5.
Both matched latest stable crate metadata checked on November 3, 2026; Indy approved these dependency targets.

[async-openai](https://crates.io/crates/async-openai) is another community SDK, but this scope needs no extra OpenAI SDK.
Start with the existing dependencies and thin typed adapters; adopt a provider crate only if source review removes meaningful code without weakening those controls.

## Version and secret boundaries

The daemon resolves decision keys from the platform-admin workspace's existing Secrets vault, selected by `PLATFORM_ADMIN_WORKSPACE_ID`.
The platform admin stores or rotates `api_key` in the native provider bag: `typesafe`, `openai`, `perplexity` or `cloudflare`.

Native API selection determines that bag; fleet messages and runner configuration cannot supply a secret name or key value.
There is no tenant-key or runner-environment fallback for decision calls.
Resolve current credential state after reserving each physical call; a missing or revoked key blocks dispatch with a named prerequisite.

Credentials never enter sandbox tools, conversation messages, saved workspace files or R2 objects.
No provider is silently selected when its profile is missing or unavailable.

Record configured and returned model identity, profile/question revision, state digest, task generation, goal revision, assessed head and physical usage.
Only model/version identities admitted by that profile's evaluation policy may drive task creation, association or goal transitions.
An unknown or imprecise version blocks native decisions; retain usage and the original message.

For TypeSafe, send `model: jev-latest` and record the returned version.
Perplexity echoes the requested string; choose an evaluated versioned model identifier rather than treating an alias echo as resolved version proof.
Another provider must pass its own identity and capability proofs; API similarity alone is insufficient.

Native code owns authority and transitions; decision models supply assessments rather than permission.

## Native probability primitives

| Primitive | Meaning | M236 use |
|---|---|---|
| Choice | Selected option, per-option probabilities and derived confidence | Intent, existing-work association and next-action proposal. |
| Noul | Probability from zero to one that a yes/no statement is true | Semantic completion and clarification questions. |
| Score | Distribution across an ordered rubric and expected score | Optional qualitative assessment only when a reviewed rubric needs it. |

A Noul near 0.5 expresses uncertainty; it is not a null or missing answer.
Noul has no separate confidence field.
Choice confidence summarizes the distribution and is distinct from the winning option's probability.

The following primitives describe native probability providers; the adapters must retain provider-specific calibration and refusal semantics.
Question identifiers are response keys; question instructions must carry the actual meaning.

Do not depend on identifier wording that TypeSafe does not send to the model.
Pin the fleet question revision and evaluated model profile together for every assessment.

## Who writes and stores the questions

The fleet owner controls routing and completion questions, including their wording and criteria.
Setup chat or CLI authoring drafts `config["x-agentsfleet"].decision_policy`; business guidance stays in `instructions_md`.
The owner reviews and applies that candidate through the existing setup flow.

The setup-chat LLM proposes questions in the first draft once the job and success conditions are clear.
It derives criteria without requiring DT or model terminology, and asks when the outcome or scope is unclear.
The decision model evaluates accepted questions during work; it does not author the setup questions.

The assistant derives questions from the owner's described outcome, scope and available evidence.
It shows the proposed question, yes/no criteria or choice criteria, and the scope of the change.
The owner can revise poor criteria through chat without editing source code or asking a platform administrator.

An owner can supply exact wording: “Add a completion question: does the repair preserve SSO login? Yes when the login regression passes; No when it fails or is missing.”
The authoring assistant preserves supplied wording and proposes missing criteria for review.

Each Apply creates another immutable fleet revision; it never edits an older revision.
New DTs adopt the current accepted policy and their specific objectives automatically.
An active DT keeps its accepted version until explicit revision adoption invalidates older assessments.

Native code owns typed request construction, allowed answer/action shapes, trusted state and mandatory authority/evidence checks.
The proposed `afr_agent/src/decision/{questions,routing,goal}.rs` renders the accepted configuration without changing its meaning.
Provider documentation and adapter verification establish which question types the selected model supports.

```text
UI setup chat / CLI -> assistant drafts instructions_md + decision_policy
                                              |
                                       owner Review/Apply
                                              |
                        R7 immutable -- new draft/Apply --> R8 immutable
                             |                                  |
                        DT accepts R7                      new DT accepts R8
                             |
                   explicit adoption of R8
                             |
            record acceptance; invalidate older assessments
                             |
   accepted questions + current evidence -> typed request -> selected model
```

Task admission saves its accepted questions in `fleet.tasks.questions`, bound to `accepted_revision_id` and `goal_digest`.
Adoption preserves prior revisions, assessments, confirmed effects and cumulative spend.
The model catalogue supplies supported types and evaluated model thresholds; fleet owners control the question text and criteria.

The native policy parser, proposal-tool schema and UI/CLI question review are written; full runtime acceptance remains pending.
Native decision requests compile the accepted policy and complete task/source state.
Task adoption and the runtime decision loop remain pending; the typed client and catalogue are separate implemented components.

`DecisionPolicy` resolves `RoutingQuestions` and `CompletionQuestions` through the existing serde/garde parser.
Each role requires its exact native criterion keys; all wording and criterion text are owner-editable.

Statements retain stable IDs and their reviewed order. Empty, duplicate, unknown, NUL-containing and oversized policy is refused, never filled with hidden questions.

Question and criterion text is at most 512 UTF-8 bytes; each complete authored question is at most 1 KiB. The actual rendered request must independently satisfy the selected provider's request bounds.
The shared [Linkwarden policy fixture](../../tests/fixtures/fleets/goal-repair/decision-policy.json) illustrates the editable representation.

## Question display in setup and task views

UI review and CLI setup output show owner-editable success criteria under `What counts as done?`, separately from instruction prose.
Show the saved setup revision, current revision, question changes and the revision each DT has accepted.
Use the existing numeric fleet revision and history rather than creating another question-version counter.

The browser task view shows accepted success criteria and the saved setup revision.
Latest assessments and assessed evidence identity remain targets pending task-runtime integration.

Label Noul values as probability of yes; they are neither a separate confidence value nor a percentage of work completed.
Preserve readable CLI output alongside the existing machine-readable representation.

Keep the owner-approved `Save done vN → Open fleet` status pill and existing design-system controls. Normal setup omits routing categories, association/decomposition rules, next-action choices and model terminology; those remain in validated configuration and machine-readable output, editable through setup chat.

The separate HTML mocks retain the current setup and fleet-chat layouts, composers and activity indicators.
Setup chat authors fleet revisions and inline success criteria; fleet chat handles work requests and DT activity.
Each fleet keeps one work conversation; DT details remain associated with it.

```text
Fleet: Linkwarden repair
Setup revision: R8 (current)

What counts as done?
  1. Does the patch address the accepted defect?
     Yes: current code and regression evidence demonstrate the fix.
     No: evidence is missing, contradictory, or the defect remains.

PR 101: accepted R7; R8 available; adoption requires an explicit change
```

## Intent routing

```text
Admitted human message + bounded relevant conversation
                      + trusted eligible DT descriptions
                               |
                        configured decision provider
                               |
conversation / status / refresh / steer / new_work / clarify
                               |
                   native association and permission checks
                               |
                answer / read / input receipt / create / clarify
```

Run routing within the existing lease or its safe input boundary; no second queue or classification worker is added.
Use trusted DT options plus explicit new-work and unclear outcomes.

Use compact option keys with an immutable native identity map in the state, so sixteen candidates fit the question-size bound.
Accept associations through that recorded map; the option key itself grants no authority.

A selected browser work item is a hint that native ownership checks must validate.

One closed-set label must not discard independent requests within a mixed message.
Preserve the original text and let the conversational model propose separate bounded actions when needed.
Native code records which input supports each action and refuses ambiguous write targets.

Social input may use one intent-routing call through the selected profile.
It makes no investigation-tool call or second goal-evaluation call solely because older work exists.
Signed machine completions skip intent routing and match recorded waits natively.

Low-confidence association, missing candidates, malformed answers and timeouts never default to new work.
Retain the admitted message and ask for clarification or show the named routing failure.
The proposed thresholds below require frozen real-message evaluation before enabling live autonomous writes.

## Goal evaluation

The accepted `decision_policy` holds user-reviewed completion questions and criteria; `instructions_md` supplies their business context.
Task admission freezes that fleet revision and the specific objective; typed templates preserve their meaning.
Question changes require a new immutable revision and explicit adoption before they can affect an existing DT.

At a completed action group or resumed-task boundary, send bounded current task state and attributable evidence.
Batch related semantic Noul questions with a Choice over CONTINUE, WAIT, DONE and BLOCKED.
Include current limits so the model sees the available choices, while native code enforces the actual bounds.

```text
Coding model -> complete action group -> native outcomes/evidence
                                             |
                                      typed decision answers
                                             |
                                  native validity/proof checks
                                   /       /      \       \
                              continue   wait    done    blocked
```

Facts already proven by native code need no probabilistic rediscovery.
Passing regression evidence and required checks must match the current PR head and approved check identities.
A model's high completion assessment cannot replace those checks.

WAIT requires a concrete registered dependency or bounded fallback read.
BLOCKED names missing permission, prerequisite or evidence; exhaustion retains cumulative limits.
Decision providers supply typed assessments; recorded facts and the conversational model provide user-facing explanations.

## Failures, cost and injection boundaries

Validate answer type, known options, finite numeric range, probability distribution and required response usage.
Reject stale generation, instruction revision or evidence binding before accepting a proposed transition.
Repository text and logs remain untrusted state and cannot replace templates, grants or native completion requirements.

Detect repeated JSON object keys before their earlier values disappear.
Ambiguous answers invalidate the assessment while retaining independently readable usage; ambiguous usage or duplicate envelopes retain unknown accounting.

Use bounded timeouts and retry accounting shared with the task or pre-task admission.
Input tokens determine TypeSafe's documented charge; output usage remains recorded even when its rate is zero.
Resolve exact rates through reviewed configuration rather than reusing an unrelated chat model's prices.

Missing usage after an uncertain request cannot be treated as free.
Retain uncertain call accounting and stop at the configured reservation ceiling until settlement can resolve it.
Timeouts, unsupported responses and provider failure never default to success or permission.

The following proposed values fix timeout, retry, state-size and threshold policy for implementation signoff.
Tests must include alias changes, uncertain routing, mixed requests, prompt injection, stale decisions and missing evidence.
No live provider acceptance is claimed by documentation review.

## Proposed question and threshold policy

The following numeric defaults belong to the proposed Jev profile, not every provider.
Perplexity, Clef and OpenAI profiles require their own evaluated thresholds; self-reported probabilities cannot inherit Jev's policy.

Use format revision `dt-routing-v1` for intent and `dt-goal-v1` for goal assessment.
Render questions from the accepted fleet policy; bind its revision and normalized digest with the DT.
Repository text cannot replace the owner's accepted questions or native guard rules.

| Question | Typed meaning and native acceptance |
|---|---|
| Intent | Choice over conversation, status, refresh, steer, new_work and clarify. Accept the selected route only when winning probability is at least 0.85 and confidence at least 0.70. |
| Target association | Choice over the eligible native DT identities plus new_work and unclear. Binding input that can change work requires probability at least 0.95 and confidence at least 0.85. |
| Multiple requests | Noul asking whether the full message requests independent actions. At most 0.30 permits single-request handling; at least 0.70 requires bounded decomposition; the open interval clarifies before effects. |
| Goal statements | One Noul per accepted completion statement, at most six. Every required semantic statement must reach 0.95 before DONE can be accepted. |
| Next action | Choice over CONTINUE, WAIT, DONE and BLOCKED. CONTINUE needs probability 0.80/confidence 0.70; WAIT needs 0.85/0.75; DONE needs 0.95/0.85 plus all native proof. |

Missing mandatory evidence makes DONE unavailable regardless of model confidence.
WAIT also requires a recorded dependency; a low-confidence answer alone cannot create an indefinite wait.
Unaccepted routing clarifies; unaccepted goal assessment records BLOCKED with preserved progress and a named uncertainty.

These numbers are proposed operating policy, not measured accuracy claims.
Freeze 60 intent cases and 40 goal/evidence cases, including mixed requests, stale heads, quotes, ambiguity and injected logs.
Before live writes, require zero unauthorized association or false completion and at least 95% expected dispositions overall.

An unknown resolved version stays blocked until the same frozen cases meet those criteria and reviewed version policy admits it.
Use the reviewed profile's configured model identifier and record returned identity; do not silently switch providers or model versions.

Question edits create another fleet revision; model-threshold edits create another reviewed model profile revision.
Either change invalidates affected cached decisions when the DT adopts it.

Each decomposed item must independently pass its intent and target thresholds before native acceptance.
More than four independent requested actions produces clarification or an explicit request to split the work; no action is silently discarded.
Tests cover exactly 0.30/0.70 and values immediately on either side, including high-confidence intent with uncertain multiplicity.

## Proposed request bounds and accounting

| Bound | Proposed value |
|---|---|
| State | 16 KiB serialized UTF-8 JSON, including complete newest message and bounded trusted summaries. |
| Questions | At most eight; authored questions stay within 1 KiB each. Each compiled question permits 16 KiB for the complete native option set; no images, long-context header or external assets. |
| Response | At most 64 KiB; exact question-key coverage and known options required. |
| Call deadline | Three-second connect timeout; 12 seconds per physical request; 30 seconds total per logical decision. |
| Retry | One retry after 429/529, at most two seconds of jittered backoff or Retry-After within the remaining deadline. |
| Unknown network outcome | No automatic retry; retain the reservation and expose decision uncertainty. |
| Reservation | USD 0.02 for each physical call, separately held before sending; cumulative task/admission bounds still apply. |
| Initial reviewed rate | 42 nanodollars per input token, zero per output token, matching the inspected TypeSafe price of USD 0.042 per million input tokens. |

Do not truncate the newest message or silently omit an eligible candidate to fit the state budget.
If bounded native summaries cannot fit, clarify or request narrower work without making a target decision.
At most 16 unfinished tasks bounds normal association candidates; all descriptors remain untrusted semantic hints until native ownership checks pass.

Use a separate measured request line for each physical Jev call, with provider, resolved model and the reviewed input/output rates.
The daemon commits the reservation and call identity before returning trusted credential access.
Renew/report settlement deduplicates by physical call identity so pre-task routing and later DT work cannot charge it twice.

The documented rate is a proposed initial configuration value; verify it before enabling the integration.
Retain a conservative reservation for missing usage until authoritative provider reconciliation can settle it; never release it merely because the client timed out.
If reported cost exceeds its reservation, record actual cost, block further paid actions and surface the overage without inventing a hard vendor-spend guarantee.

No observed provider tokenization proves the reservation is an absolute charge ceiling.
The bounded request, reviewed rate and per-call reservation limit local admission; actual provider usage remains the accounting authority.
Price/version policy changes block new calls until reviewed rates and evaluation evidence are updated.

Use `reqwest` 0.13.5 through existing transport conventions, with redirects disabled and secrets redacted.
Native decision adapters share guarded transport and bounded accounting; existing Rig coding codecs remain unchanged.

### Billing alongside coding calls

Existing `RequestUsage` carries token counts under one lease-wide coding-model rate; that alone cannot price a different decision provider.
Add optional `decision_call_id` to renew/report usage references and resolve its immutable profile/rate snapshot from the daemon reservation.
A decision receipt creates one canonical billing line; PATCH, renew and report reference that identity rather than creating additional charges.

The daemon validates tokens and usage against the physical call and uses only its authorized rates, never runner-supplied prices.
Keep a decision-protocol price schedule beside the catalogue entry/profile; the same model can have different coding and decision rates.
Coding usage retains its existing price schedule; the decision line uses its reserved protocol/profile revision.

Normalize reported billable input, output, cached-read and cache-write usage exactly once.
Perplexity Flash's per-question state accounting must not multiply input tokens that the provider already aggregated.
Retain the original usage categories and normalize them through tested provider fixtures before applying the authorized decision schedule.

Unknown calls retain their reservation; replacement or task binding cannot charge the same physical call again or reset cumulative limits.

### Native billing integration

Use the original lease's `WorkKey::FleetLease`; binding an intent call to a later task does not create another billing identity.
The call record retains its runner, original admission/lease, optional task, authorized generation, native usage sequence, immutable profile/price snapshot and reservation.
Coding usage keeps `[0, 2^31)`, runtime uses `[2^31, 3 * 2^30)`, and native decision calls use `[3 * 2^30, 2^32)`; both native allocators refuse exhaustion.

Runtime-line counts in `meter_lease.sql` and `bill_expired_slice.sql` must exclude decision sequences.

Reuse `afd_billing::price::{PriceBook, Rater}` with frozen decision tiers and platform posture.
Do not pass decision usage through the coding lease's `usage::Pricing`; distinct provider/model/posture lines produce separate settlements under the same work identity.
The existing billing sweeper groups those three attributes with work identity.

Native reservations and PATCH receipts retain original ownership; renew/report receipt integration remains pending.

### Running task authority and source attribution

A routing call can assess human input B while task A holds the fleet's active lease.
The wire task stamp still proves A's current generation, but intent charges remain attributable to B's source admission.
Only a goal call carries A as its task-budget owner; both retain the issuing lease's original billing work identity.

A source can be that lease's own admission or another eligible human input claimed under the same lease/fence.
Claiming preserves the original message and does not accept an interpretation.
Later task binding cannot transfer or duplicate the routing charge.

Up to ten physical calls cover one whole-message assessment and four item assessments, each with at most one permitted retry.
At two cents each, the conservative source reservation allowance is USD 0.20; smaller fleet, lease and credit limits still win.
Pending reservations consume capacity; issued calls and unknown outcomes remain counted across replacement leases and later binding.

This ceiling does not permit ten repeated whole-message classifications.
Each logical assessment has a retained identity, immutable request snapshot and thirty-second deadline.
One retry references its original physical call, preserves that assessment snapshot/deadline, and receives a distinct physical identity and usage line.

Save bounded HTTP status and delay evidence; only 429/529 may permit the one retry.
A delay over two seconds, an expired logical deadline or an unknown transport outcome refuses retry.
No provider error body or raw header is retained as retry evidence.

Resolve operation replay before rendering current state or reevaluating counters.
Identical issued operations return their existing identity without credentials; changed request content conflicts.
A reserved operation can complete its first issuance after confirmed funding, with one native compare-and-set winner.

Late receipts authenticate the original runner, lease and fence, even when the current lease/task has changed or disappeared.
Accounting and semantic usability remain separate; a stale receipt can settle its original charge but cannot advance work.
The first unknown failure cannot be rewritten into throttling evidence; authoritative known usage may later reconcile it.

A hold ceiling is absolute and monotonic, so repeating a two-cent ceiling cannot reserve another physical call.
Compute the requested ceiling from all logged charges, current coding headroom and all unresolved physical-call reservations.
The native funding gate refuses an unpriceable coding headroom; self-managed coding supplies a known zero headroom while platform decisions still require credit.

Save call identity before releasing the connection for `Biller::reserve`, then recheck current authority and aggregate coverage before credential dispatch.
Only confirmed `Hold::Held` admits a new paid decision; an unconfirmed or absent wallet cannot authorize provider spend.
`dt_decision_funding_refused` distinguishes missing coding prices, insufficient credit, closed work and unconfirmed credit through bounded reasons.

Resolve the platform-admin secret freshly after confirmation; missing or revoked keys do not select a tenant or environment substitute.

One receipt recorder serves PATCH, renew and report.
It authenticates the original call owner, validates usage once, and commits call accounting plus `ledger::log_lines` in the same transaction; settlement follows commit.
Identical receipts replay; conflicting known usage fails.

An unknown result creates no zero-cost placeholder that could consume the actual billing sequence.
Record attributable late usage before rejecting stale work; accounting does not grant current task-transition authority.

Report and sweep release the whole lease hold only after all physical calls are reconciled, including calls whose original lease is no longer active.
The existing whole-hold API conservatively retains excess headroom until then; it cannot lower a live ceiling safely.

## Concrete repair decision example

This request illustrates an assigned bookmark-import repair; it is not a live Linkwarden observation or Jev result.
The conversational model writes the patch; native code supplies evidence identities and actual check results.
Fields inside `state` are our application data; `model`, `state` and `questions` are TypeSafe request fields.

```json
{
  "model": "jev-latest",
  "state": {
    "accepted_request": "Fix the missing-title import crash in PR 101. Preserve the regression. Do not merge.",
    "original_failure": "Calling title.trim() throws when the imported title is missing.",
    "current_head": "head-B",
    "patch_excerpt": "const title = (input.title ?? '').trim();",
    "regression": {"head": "head-B", "unchanged": true, "result": "passed"},
    "required_checks": {"head": "head-B", "unit": "passed", "integration": "pending"}
  },
  "questions": {
    "repair_matches_request": {
      "type": "noul",
      "instructions": "Does the repair address the missing-title crash while preserving the accepted scope?",
      "criteria": {
        "true": "Missing titles are handled; normal title trimming and the original regression remain; no unrelated change or merge is proposed.",
        "false": "The crash remains, ordinary title behavior changes without authorization, the regression is weakened, or unrelated actions replace the repair."
      }
    },
    "next_action": {
      "type": "choice",
      "instructions": "Choose the next action from the accepted goal and current evidence. Required checks must finish successfully before completion.",
      "criteria": {
        "CONTINUE": "Concrete corrective work remains and current authority and limits permit it.",
        "WAIT": "A known required check is still running; no additional patch is currently justified.",
        "DONE": "All accepted goal conditions have current evidence, including every required check at the current head.",
        "BLOCKED": "A missing prerequisite, unresolved contradiction, permission or exhausted limit prevents authorized progress."
      }
    }
  }
}
```

If the semantic assessment meets its threshold, the expected disposition is WAIT because integration remains pending.
Even a high completion probability cannot bypass that native check.
Show `Checking goal progress (<configured provider/model>)` during evaluation, then the confirmed dependency after native settlement.

Intent routing instead sends the newest complete message and eligible DT summaries.
For `Use the smaller fix for PR 101`, Choice criteria distinguish steering existing work from new work and status questions.
Native association still validates the selected task and source message before accepting that input.

## Transport scope

The inspected manifest and lockfile use reqwest 0.13.5, rustls 0.23.46 and aws-lc-rs 1.19.0.
Those versions matched stable crate metadata inspected on November 2, 2026.
M236 reuses the existing HTTPS certificate, platform-trust and proxy behavior with redirects disabled for decision requests.

Indy assigned quantum-safe transport to a separate specification.
Do not change key-exchange preferences, add a crypto provider or claim end-to-end quantum safety under M236.
AES-256-GCM checkpoint encryption at rest remains part of M236.

### Memory and allocation boundaries

The decision client owns one `SecretString`, whose `Zeroizing<String>` buffer is wiped when dropped normally.
The configured client has no `Clone` implementation; composition can share its existing behavior trait through `Arc`.
HTTP headers and transport buffers remain outside that owned-buffer wiping guarantee.

Build the final bearer-authenticated request once before dispatch, preserving builder failures as `NotSent` outcomes.
Deserialize response values by reference and count validation bytes through a bounded writer.
The decision-local parser records duplicate keys during its single parse, preserving unambiguous accounting without another document copy.

The object-safe evaluator uses one boxed future per call; native wire adapters use concrete enum dispatch.
Bounded metrics and stage timing support later regression investigations.
Production latency, allocation counts and large-repository transfer performance remain unmeasured until the declared performance scenarios run.
