# Fleet definitions, revisions, and Library

Read this when changing reusable setup, fleet setup revisions, or Definition validation. Exact wire types live in [API resources](api_resources.md). Creation and recovery live in [data flow](data_flow.md#fleet-creation-and-setup-recovery).

| Object | Authority and lifetime |
|---|---|
| Private draft | Creator-owned chat and mutable candidate selection; only its creator can read private text. |
| Candidate | Immutable normalized Definition with a daemon-derived identity, revision and digest. |
| Library entry | Explicitly published reusable revisions, visible to its workspace or platform audience. |
| Fleet | Independent runtime instance with its own setup revision, approvals, history, memory and schedules. |

## Setup authority

One validated Definition serves browser chat, terminal chat, review, Library publication and runtime setup. The typed implementation is in `rustd/crates/afd_fleet_runtime/src/definition/`. End-to-end and migration proofs remain pending under the active cutover spec.

### Drafts, Library and fleets

A draft may publish V1 of a Library entry and continue editing. Publishing to that entry appends V2; publishing as a new entry creates a separate Library identity. A fleet created from V1 keeps V1 until an authorized person explicitly applies a reviewed improvement. A new install may select V2.

Save creates a fleet and closes its setup draft after successful activation. Save never publishes to Library. Improve starts a new private draft pinned to the fleet's current setup. Cancelling setup returns its linked draft to editing; retry uses the recovery rules in data flow.

### Explicit Library publication

Publishing is a separate human action against the reviewed candidate. It either creates a Library entry or appends an immutable revision to an explicitly selected entry. A failed or retried publication cannot create a second revision for the same operation. Publication does not create a fleet, close its source draft, or change any installed fleet.

## Storage and runtime reads

Postgres stores normalized Definitions, immutable candidate/Library/fleet revisions, and content-free replay identities. Runtime leases receive instructions and parsed policy from the fleet's pinned revision. Library removal hides the entry while retaining revisions referenced by installed fleets. No object-store bundle is needed for this path.

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

Drawer and model proposal tool use the same bounded edit operations. The first model proposal may initialize a full definition only when no candidate exists; first drawer mutation sends a typed definition source with explicit shown defaults. An editable definition can have empty instructions; Save/publication refuses it until complete. There is one configuration schema and validator. Configuration operations are JSON Patch `add`, `remove`, or `replace` below `/config`, plus `replace` of `/description`, a leaf that is never added or removed; preserve array order. Instruction edits are non-overlapping UTF-8 byte ranges `{start_byte,end_byte,text}` against the exact candidate revision. Validate boundaries before editing. Apply the entire batch to a copy, validate once, and replace it under a version guard. Drawer uses If-Match; runner proposals name both base candidate ID and revision. Each accepted change gets a fresh immutable ID and next revision. Lease and proposal receipts expose those versions; exact retry returns its original accepted version. A stale or invalid batch changes nothing.

First generated instructions are at most 12,288 characters; later edited instructions at most 32,768 characters. These character ceilings never replace L9's tokenizer checks. Count system instructions, tools, history, and reserved output against the model window. Long instructions are changed through small edits rather than repeated full rewrites.

### Immutable revision and human review

The daemon derives candidate identity, normalized definition, `canonical_digest`, `schema_version`, and `policy_semantics`. The model sets none of these. Digest v1 uses recursively sorted object keys, preserved array order, normalized finite-number JSON rendering, LF text and UTF-8 bytes, with an explicit version prefix. Pin key ordering and numeric rendering through fixtures. Money is already a decimal string and never rounded through a float. Unsupported versions or executable setup stop before Save.

Review binds candidate content, instance name, runtime model, source/target revisions, policy marker, and relevant resolution versions. Exclude `readiness[].observed_at`: ordinary time passage must not invalidate every review. Changed effective permissions or model resolution invalidates the digest. Missing connections change live readiness without rewriting reusable requirements. Save rechecks the workspace owner's authority.

A Library revision holds reusable setup and immutable requirements. A fleet separately owns its setup revision, approvals, history, memory, sessions, overrides, and schedules. Applying an improvement does not publish it. Publishing a Library revision is an explicit separate action.

### Native storage and terminal chat

Chat is the authoring path in both the browser and terminal. The daemon stores typed setup and instructions in native revisions. Authoring reads and writes no `SKILL.md`/`TRIGGER.md` files, bundle manifests, support files, tar snapshots or import/export adapters. To reuse an older prompt, paste it into a draft chat and publish the reviewed result. Runtime repository access is configured policy and remains subject to grants; it is separate from importing a fleet definition.

Serialization round trips cover the description, gates, anomaly rules, network read_only/read_post_paths, repository writes/base, context, trigger options, monthly budgets, and the runtime model reference. A missing description and uppercase/spaced slugs fail. Initialize → edit → edit, lost acknowledgements, stale/reordered proposals, and a small edit to 32 KiB instructions must work without retransmitting the definition.

The terminal client creates or resumes a private draft and sends ordinary chat messages. It displays the same reviewed candidate and readiness checks before explicit Save or Library publication. Reusing a Library revision creates an independent fleet with that revision pinned. Library updates never propagate to existing fleets without Improve and explicit Apply.

Remove dollar budget keys from live readers, writers, generated API, CLI, app, and active fixtures. New fields are positive checked integer nanos with the existing daily/monthly ceiling limits, decimal strings on the wire. Keep frozen migration history and completed milestone records as historical evidence. Add a new migration; the release plan resets acceptance-test setup data before traffic resumes rather than guessing conversions.
