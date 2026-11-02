# Fleet definitions, revisions, and Library

Read this when changing reusable setup, fleet setup revisions, or Definition validation. Exact wire types live in [API resources](api_resources.md). Creation and recovery live in [data flow](data_flow.md#fleet-creation-and-setup-recovery).

| Object | Authority and lifetime |
|---|---|
| Private draft | Creator-owned chat and mutable candidate selection; only its creator can read private text. |
| Candidate | Immutable normalized Definition with a daemon-derived identity, revision and digest. |
| Library entry | Explicitly published reusable revisions, visible to its workspace or platform audience. |
| Fleet | Independent runtime instance with its own setup revision, approvals, history, memory and schedules. |

## Setup authority

The proposed [goal-driven repair scenario](scenarios/durable-repair.md) stores user-edited goals and completion questions in reviewed `instructions_md`. Jev runtime evaluation and instruction-revision adoption for active tasks remain design work; current prose does not grant new capabilities.

One validated Definition serves browser chat, terminal chat, review, Library publication and runtime setup. The typed implementation is in `rustd/crates/afd_fleet_runtime/src/definition/`. Observed checks and remaining live proof belong in the [verification ledger](shared_chat_verification.md#historical-verification-evidence).

### Drafts, Library and fleets

A draft may publish V1 of a Library entry and continue editing. Publishing to that entry appends V2; publishing as a new entry creates a separate Library identity. A fleet created from V1 keeps V1 until an authorized person explicitly applies a reviewed improvement. A new install may select V2.

Save creates a fleet and closes its setup draft after successful activation. Save never publishes to Library. Improve starts a new private draft pinned to the fleet's current setup. Cancelling setup returns its linked draft to editing; retry uses the recovery rules in data flow.

### Explicit Library publication

Publishing creates a Library entry or appends an immutable revision to a selected entry. Retry the same key and body within the [authoring replay window](api_resources.md#shared-wire-rules). Publication creates no fleet, leaves its draft open and changes no installed fleet.

## Storage and runtime reads

Postgres stores normalized Definitions, immutable candidate/Library/fleet revisions, and content-free replay identities. Runtime leases receive instructions and parsed policy from the fleet's pinned revision.

Setup saves, readiness, access storage and memory-sharing changes add history-only receipts to `fleet.events` in their mutation's transaction. Their `recorded` status creates no admission and stays outside runtime model history; the browser displays permitted metadata in chronological order.

Library removal deletes the entry and its Library history. Installed fleets retain their own frozen revisions; draft source and publication references become null.

No object-store bundle is needed for this path.

Bundle upload, GitHub archive import, export, tar snapshots, runner bundle download, the `SKILL.md`/`TRIGGER.md` parser and source-Markdown setup writers are removed. Shared tools, vault storage, connector ingress and repository checkout keep their independent uses. `fleet.fleets` (`schema/1500_fleet_fleets.sql`) has no instruction or bundle columns, so a fleet row carries no instructions of its own; they live only in its pinned revision (`fleet.revisions`).

## Validated Definition

### One setup authority

```json
{
  "schema_version": 1,
  "description": "Explain checkout failures with evidence.",
  "instructions_md": "Investigate failures. Link evidence before proposing a change.",
  "config": {
    "name": "checkout-watch",
    "x-agentsfleet": {
      "triggers": [],
      "tools": [],
      "credentials": [],
      "budget": {"daily_nanos": "1000000000", "monthly_nanos": "10000000000"}
    }
  }
}
```

`Definition::parse` accepts this JSON and nothing wider: the four members above are the whole Definition, and any other top-level key is refused. The fleet's slug is `config.name` alone. The human title “Checkout watch” belongs to draft/review instance naming, separately from that slug. Instance allocation uses the existing validated fleet-name rules; a display title cannot relax parser validation. Runtime model selection stays in `config["x-agentsfleet"].model`; draft `runtime_model_ref` edits that field. Authoring model is separately pinned per turn and never becomes a hidden runtime selection.

| Field | Validation |
|---|---|
| `schema_version` | Required; only `1`. |
| `description` | Required display text, nonempty, at most 32,768 characters, no NUL; CRLF and lone CR normalize to LF. Absent or empty refuses as missing; null or a non-string refuses as a shape. Nothing that decides what a fleet may do reads it. |
| `instructions_md` | Required string, at most 32,768 characters, no NUL; CRLF and lone CR normalize to LF. May be empty while editing. |
| `config.name` | Required lowercase letters/digits/hyphens, 1..64 bytes. |

Evidence: `rustd/crates/afd_fleet_runtime/src/definition/mod.rs:63–118` and `src/name/mod.rs:37–67`. Runner tags are not setup: no level of the Definition accepts `tags`, and placement requires none. Browser and terminal chat use the same validator.

Retain approval gates, anomaly rules, monthly ceilings, repository write access, network `read_only`/`read_post_paths`, trigger options, and context limits. Reject model-generated grants, secret values, provider identities, permission receipts, and unknown configuration keys. Credential names and repository requirements retain their existing meaning. `triggers` lists what wakes the fleet besides the messages sent to it; it may be empty or omitted for a fleet that is only ever talked to, and is stored as `[]` either way. Credential aliases and independently modeled child roles are deferred.

### Small atomic edits

Native edits apply atomically to one candidate revision. Configuration operations use JSON Patch `add`, `remove`, or `replace` below `/config`, plus replacement of `/description`; array order is preserved. Instruction ranges use UTF-8 byte offsets against the unmodified instructions.

The drawer supplies native edits with `If-Match`; runner proposals name both candidate ID and revision. Invalid boundaries, overlap, stale versions or invalid final setup change nothing. Accepted changes create a fresh immutable candidate ID and next revision; exact activity retries return their originally accepted version.

The model instead proposes `append {text}` or `replace {old_text,text}` instruction edits. The daemon resolves exact text against the pinned candidate and converts unique matches into native byte ranges. Missing or ambiguous matches become typed refusals; the model never counts Unicode bytes.

The runner returns bounded corrective feedback for refused arguments or rejected native edits. Corrections share the admitted turn’s output allowance and maximum eight model calls; exhaustion ends without claiming the setup is ready. Usage remains charged for observed calls, including rejected proposals.

An accepted edit retains the model's conversational answer. A tool-only reply continues with its accepted receipt within the same allowance; the runner supplies no stock review reminder. Questions can be answered without proposing another edit.

Only a missing candidate permits full initialization. First drawer input uses a typed Definition with visible defaults. Empty instructions remain editable but block Save and publication.

Implementation: `afd_authoring/src/model/provider/edits.rs`, `afd_fleet_runtime/src/definition/edit/mod.rs`, and `afr_supervisor/src/authoring/{proposal,turn}.rs` under `rustd/crates`.

First generated instructions are at most 12,288 characters; later edited instructions at most 32,768 characters. These character ceilings never replace L9's tokenizer checks. Count system instructions, tools, history, and reserved output against the model window. Long instructions are changed through small edits rather than repeated full rewrites.

### Immutable revision and human review

The daemon derives candidate identity, normalized definition, `canonical_digest`, `schema_version`, and `policy_semantics`. The model sets none of these. Digest v1 uses recursively sorted object keys, preserved array order, normalized finite-number JSON rendering, LF text and UTF-8 bytes, with an explicit version prefix. Pin key ordering and numeric rendering through fixtures. Money is already a decimal string and never rounded through a float. Unsupported versions or executable setup stop before Save.

Review binds candidate content, instance name, runtime model, source/target revisions, policy marker, and relevant resolution versions. Exclude `readiness[].observed_at`: ordinary time passage must not invalidate every review. Changed effective permissions or model resolution invalidates the digest. Missing connections change live readiness without rewriting reusable requirements. Save rechecks the workspace owner's authority.

In browser authoring, ordinary questions and changes continue the conversation without a review reminder. A standalone command such as “save this” or “save changes” starts one finalization-and-save operation. The selected model receives the working conversation context and current setup; it incorporates the person's latest decisions through validated native edits when needed. Exploratory questions do not become requirements. The browser shows “Saving setup...” while finalization and the checked Save or Apply write are pending, then “Save done vN” after the database acknowledges the write. Readiness is displayed separately, so saving does not claim connected access or start a run.

The save uses the candidate pinned by that completed authoring report, including a candidate first created or changed by the Save turn. A different head after finalization, failed or stopped turns, unknown usage, quoted commands and ordinary questions cannot trigger a save. Fresh digest and setup-tag validation and the existing idempotency key protect the write. The command remains pending only in the current browser session; reloading history never executes it. There is no second review or Save confirmation.

The fleet chat model selector saves a setup revision through the native edit and reviewed Apply path. Future chat, webhook and scheduled work use that runtime model. Saving sends no authoring message and preserves unsent conversation text and private setup edits.

One `model_changed` event commits with the revision and appears as “Model changed” in the fleet conversation. Replaying the same save adds no second notice.

Saved fleets use current readiness checks when activating or resuming. A different review digest, including catalogue or price changes, does not add a repeat-review hold. Explicit new saves still validate their review digest and setup tag before writing.

A Library revision holds reusable setup and immutable requirements. A fleet separately owns its setup revision, approvals, history, memory, sessions, overrides, and schedules. Applying an improvement does not publish it. Publishing a Library revision is an explicit separate action.

### Native storage and terminal chat

Private drafts are the authoring path in browser and terminal. [Source selection](user_flow.md#80-the-wedge-surface) covers blank, native JSON, Library and fleet inputs. The daemon stores typed setup and instructions in native revisions.

Authoring does not load `SKILL.md`, `TRIGGER.md`, bundle manifests or tar snapshots. Paste older prompts into chat or supply native JSON, then review before publication. Runtime repository access remains separate and requires grants.

Serialization round trips cover the description, gates, anomaly rules, network read_only/read_post_paths, repository writes/base, context, trigger options, monthly budgets, and the runtime model reference. A missing description and uppercase/spaced slugs fail. Initialize → edit → edit, lost acknowledgements, stale/reordered proposals, and a small edit to 32,768-character instructions must work without retransmitting the definition.

The terminal client creates or resumes a private draft and sends ordinary chat messages. It displays the same reviewed candidate and readiness checks before explicit Save or Library publication. Reusing a Library revision creates an independent fleet with that revision pinned. Library updates never propagate to existing fleets without Improve and explicit Apply.

Remove dollar budget keys from live readers, writers, generated API, CLI, app, and active fixtures. New fields are positive checked integer nanos with the existing daily/monthly ceiling limits, decimal strings on the wire. Keep frozen migration history and completed milestone records as historical evidence. Add a new migration; the release plan resets acceptance-test setup data before traffic resumes rather than guessing conversions.

### Webhook declarations

Registry sources `github`, `linear` and `slack` imply their signing recipe and
reject an override. A custom source requires `verifier.scheme` from
`body-hex-sha256`, `body-hex`, `body-base64-sha256`, `slack-v0` or `svix-v1`.
The three body recipes require `verifier.header`, an HTTP token of 1–128
characters.

Slack and Svix use fixed headers and reject that field.

Each trigger's `credential_name` selects a workspace credential; the name defaults
to its source. Every credential is a JSON object with a `webhook_secret` string,
including the `whsec_` value for Svix. Save returns one
`/v1/webhooks/{fleet_id}/{source}` URL per trigger.

A fleet can mix registry and
declared recipes without ordering its triggers for routing.
