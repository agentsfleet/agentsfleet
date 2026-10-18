---
type: explanation
audience: contributor
verified: 2026-10-15
product_version: 0.61.0
executable: false
---

# The definition a chat reviews and saves

## What it is

A proposed definition wraps existing validated fleet configuration with instructions and a schema version. It introduces no second policy schema. Implementation is pending under [M227](../../v2/done/M227_001_P0_API_CLI_DOCS_UI_WORKSPACE_CHAT_CUTOVER.md).

```text
private draft --> validated candidate --> human review
                                          |       |
                                     Save fleet   Save to library
                                          |       |
                                     fleet setup  immutable library revision
                                          |
                                     runtime config + instructions
browser chat + terminal chat --> one JSON parser --> same definition
```

## Why it exists

`rustd/crates/afd_fleet_runtime/src/config/mod.rs:80` defines `FleetConfig`; `:129` parses its JSON document through typed validation. `config/raw/document.rs:42–98` covers gates, credentials, network path rules, budgets, repository access, model, and context. Reusing these fields preserves their full meaning. L20 changes budget units throughout every reader; this change requires schema and client work.

## How it behaves

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

`Definition::parse` accepts this JSON. `config.name` is the fleet's only name, an authored lowercase slug. The human title “Checkout watch” belongs to draft/review instance naming, separately from that slug. Instance allocation uses the existing validated fleet-name rules; a display title cannot relax parser validation. `description` is display text that every listing shows; nothing that decides what a fleet may do reads it. Runtime model selection stays in `config["x-agentsfleet"].model`; draft `runtime_model_ref` edits that field. Authoring model is separately pinned per turn and never becomes a hidden runtime selection.

| Member | Validation |
|---|---|
| `schema_version` | Exactly `1`. |
| `description` | Required nonempty string, at most 32,768 characters, no NUL; line endings stored as LF. |
| `instructions_md` | String, at most 32,768 characters, no NUL; line endings stored as LF. Save and publication refuse it empty. |
| `config` | The existing `FleetConfig` document; `config.name` is lowercase letters/digits/hyphens, 1..64 bytes. |

Evidence: `rustd/crates/afd_fleet_runtime/src/definition/mod.rs` (`Raw`, `Definition::from_value`) and `src/name/mod.rs`. Any other member is refused by name, including `skill`, and `tags` is refused at every level; nothing unsupported is silently presented as preserved.

Retain approval gates, anomaly rules, monthly ceilings, repository write access, network `read_only`/`read_post_paths`, trigger options, and context limits. Reject model-generated grants, secret values, provider identities, permission receipts, and unknown configuration keys. Credential names and repository requirements retain their existing meaning. Credential aliases and independently modeled child roles are deferred.

### Small atomic edits

Drawer and model proposal tool use the same bounded edit operations. The first model proposal may initialize a full definition only when no candidate exists; first drawer mutation sends a typed definition source with explicit shown defaults. An editable definition can have empty instructions; Save/publication refuses it until complete. There is one configuration schema and validator. Configuration operations are JSON Patch `add`, `remove`, or `replace`, rooted at `/config`, plus `replace` of `/description`; preserve array order. Instruction edits are non-overlapping UTF-8 byte ranges `{start_byte,end_byte,text}` against the exact candidate revision. Validate boundaries before editing. Apply the entire batch to a copy, validate once, and replace it under a version guard. Drawer uses If-Match; runner proposals name both base candidate ID and revision. Each accepted change gets a fresh immutable ID and next revision. Lease and proposal receipts expose those versions; exact retry returns its original accepted version. A stale or invalid batch changes nothing.

First generated instructions are at most 12,288 characters; later edited instructions at most 32,768 characters. These character ceilings never replace L9's tokenizer checks. Count system instructions, tools, history, and reserved output against the model window. Long instructions are changed through small edits rather than repeated full rewrites.

### Immutable revision and human review

The daemon derives candidate identity, normalized definition, `canonical_digest`, `schema_version`, and `policy_semantics`. The model sets none of these. Digest v1 uses recursively sorted object keys, preserved array order, normalized finite-number JSON rendering, LF text and UTF-8 bytes, with an explicit version prefix. Pin key ordering and numeric rendering through fixtures. Money is already a decimal string and never rounded through a float. Unsupported versions or executable setup stop before Save.

Review binds candidate content, instance name, runtime model, source/target revisions, policy marker, and relevant resolution versions. Exclude `readiness[].observed_at`: ordinary time passage must not invalidate every review. Changed effective permissions or model resolution invalidates the digest. Missing connections change live readiness without rewriting reusable requirements. Save rechecks the workspace owner's authority.

A Library revision holds reusable setup and immutable requirements. A fleet separately owns its setup revision, approvals, history, memory, sessions, overrides, and schedules. Applying an improvement does not publish it. Publishing a Library revision is an explicit separate action.

### Native input and storage

Chat is the only authoring input. One JSON parser, `Definition::parse` (`rustd/crates/afd_fleet_runtime/src/definition/mod.rs`), owns validation and limits for drafts, Save and Library publication. The daemon stores typed setup and instructions in immutable revisions. It has no Markdown parser, file, GitHub or template importer, export renderer, or support-file storage; the [chat-only amendment](decisions.md#chat-only-authoring-amendment) removed them. To reuse an older prompt, paste it into a draft chat and publish the reviewed result to the Library.

Round-trip fixtures cover the description and every policy field: gates, anomaly rules, network read_only/read_post_paths, repository writes/base, context, trigger options, monthly budgets and model references. A missing description, a `skill` member, `tags`, and uppercase/spaced slugs fail. Initialize → edit → edit, lost acknowledgements, stale/reordered proposals, and a small edit to 32,768-character instructions must work without retransmitting the definition.

Remove dollar budget keys from live readers, writers, generated API, CLI, app, and active fixtures. New fields are positive checked integer nanos with the existing daily/monthly ceiling limits, decimal strings on the wire. Keep frozen migration history and completed milestone records as historical evidence. Add a new migration; the release plan resets acceptance-test setup data before traffic resumes rather than guessing conversions.

## Limits

Credential aliases, new child-role schemas, and automatic repository publication are outside scope. A policy marker detects changed semantics but cannot freeze deployed code. A mismatch blocks new work until a human applies a reviewed revision. Existing leases retain captured setup, subject to live revocation and renewal budget ceilings.

## Related pages

- [Exact resource types](../../architecture/api_resources.md)
- [Creation](../../architecture/data_flow.md#fleet-creation-and-setup-recovery)
- [Runtime preservation](runtime-baseline.md)
