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
Reuse `afd_inference::transport`, native vault resolution, usage reporting and the current lease budget.

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

One object-safe `DecisionEvaluator` trait exposes one bounded asynchronous `evaluate` call.
Use existing boxed-future conventions; no plugin registry, provider discovery service or second agent loop is needed.
`DecisionRequest` carries purpose, bounded state and reviewed questions; trusted generation/evidence bindings remain outside model-controlled answers.

`DecisionResult` carries typed answers, probability origin, provider/model identity, profile revision and physical usage.
`DecisionCapabilities` declares supported question types, distributions and refusal behavior.
Unsupported capabilities refuse activation instead of inventing probabilities or silently dropping questions.

One `DecisionClient` implements the trait with four small native API adapters.
Share question/answer representations only where provider fixtures prove compatibility; translate envelopes, authentication, usage and refusal semantics explicitly.
OpenAI's `predicate` maps to internal yes-probability semantics; its question/answer arrays differ from TypeSafe's keyed objects.

No structured-chat decision implementation or extra OpenAI SDK is needed for this scope.
Existing Rig remains the coding-provider dependency; the decision client reuses guarded HTTP and native accounting directly.

## Configuration per runner

Extend the existing runner assignment with nullable `decision_model_id`, selecting an administrator-reviewed entry in the current model catalogue.
Add decision transport/policy metadata beside that entry; do not create another model registry or put model policy in host environment variables.
The existing runner edit and heartbeat carry the assignment, and lease issuance freezes its resolved `DecisionProfile`.

The profile binds provider, model, protocol, allowed endpoint, vault reference, capability set, rates, question-policy revision and evaluated model versions.
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

The workspace already pins Rig 0.45.0 and reqwest 0.13.5.
Indy approved upgrading Rig to latest stable 0.46.0.
Reqwest 0.13.5 remains current; crate metadata was checked on November 3, 2026.

[async-openai](https://crates.io/crates/async-openai) is another community SDK, but this scope needs no extra OpenAI SDK.
Start with the existing dependencies and thin typed adapters; adopt a provider crate only if source review removes meaningful code without weakening those controls.

## Version and secret boundaries

The daemon resolves the selected profile's vault reference into the trusted provider plane.
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
Keep question templates and threshold policy together in the reviewed decision module.

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

The accepted `instructions_md` holds user-reviewed completion questions.
Task admission freezes the instruction revision; typed templates render those questions without changing their meaning.
Question changes require explicit revision adoption and invalidate stale answers.

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

Use question revision `dt-routing-v1` for intent and `dt-goal-v1` for goal assessment.
Render task completion statements from the accepted reviewed instructions; store their normalized digest with the DT.
Never substitute repository-provided instructions for the question templates.

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
Changes to thresholds or wording create another question revision and invalidate cached decisions.

Each decomposed item must independently pass its intent and target thresholds before native acceptance.
More than four independent requested actions produces clarification or an explicit request to split the work; no action is silently discarded.
Tests cover exactly 0.30/0.70 and values immediately on either side, including high-confidence intent with uncertain multiplicity.

## Proposed request bounds and accounting

| Bound | Proposed value |
|---|---|
| State | 16 KiB serialized UTF-8 JSON, including complete newest message and bounded trusted summaries. |
| Questions | At most eight; at most 1 KiB each; no images, long-context header or external assets. |
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
