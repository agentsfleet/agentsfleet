# Goldfish Review Report

**Target:** [M249_003](/workspace/agentsfleet-m249/docs/v2/pending/M249_003_P1_API_CLI_DOCS_UI_PLUGIN_AND_ARCHIVED_VOCABULARY.md), full file, reviewed lines 1–297.

**Review:** Elephant-Goldfish Model (EGM), Stage 9 implementation readiness, one round.

**Verdict:** Not ready for implementation. The completed reviews produced four distinct Major findings and four Nits; no Blockers.

**Reviewed snapshot SHA-256:** `b28d2209cb746a50d2b4630122a6bd5582b8eb05d73ca63c15c4d1b5387fb6fb`.

Target line references below describe that snapshot. The root agent began preparing amendments after reviewers finished their source checks. Those amendments are outside this review.

## Reviewer Status

| Reviewer | Status | Required reads | Reported findings |
| --- | --- | --- | --- |
| `implementation_readiness` | completed | Persona, integration guide, repository instructions, complete target and implementation-readiness standard | 3 Major |
| `codebase_details` | completed | Persona, integration guide, repository instructions, complete target and implementation-readiness standard | 3 Major |
| `style` | completed | Persona, integration guide, repository instructions, complete target, documentation and spec standards | 4 Nit |

Each reviewer ran in a fresh context with `fork_turns: none`. Every reviewer returned a final report and confirmed the required reads.

The two technical reviewers independently reported the scope-inventory and daily-count findings. Deduplication reduces their six Major reports to four distinct findings.

Reviewers inspected source and documentation without edits, tests, builds or git commands. The synthesizer wrote only this report.

## Prioritized Findings

### Existing browser tokens lack a transition procedure - Severity: Major

* **ID:** `implementation_readiness-1-1`
* **Reviewer Lens:** `implementation_readiness`
* **Affected Scope:** Target §1a, lines 112–117.
* **Discrepancy Description:** Empty resolver caches do not refresh claims embedded in existing browser tokens. The renamed parser ignores their former `library:write` token. The plan covers newly issued sessions but does not specify how existing writer sessions retain access when traffic resumes.
* **Evidence:** [Session-token authentication](/workspace/agentsfleet-m249/rustd/crates/daemon/afd_auth/src/authenticate.rs:291) reads the token claim directly and bypasses the capability resolver. [Claim parsing](/workspace/agentsfleet-m249/rustd/crates/daemon/afd_auth/src/scope.rs:411) ignores unknown scope strings.
* **Rationale & Recommendation:** Specify the renewal or expiry procedure before resuming traffic. Include a token issued before metadata replacement in the deployment checks. Preserve the approved exact replacement and narrowed-reader negatives.

### Daily activity counts cannot satisfy unrestricted additive reconciliation - Severity: Major

* **IDs:** `implementation_readiness-1-2`, `codebase_details-1-3`
* **Reviewer Lens:** `implementation_readiness`, `codebase_details`
* **Affected Scope:** Target §6, lines 198–200 and Dimension 6.1 at line 213.
* **Discrepancy Description:** Daily entries reuse the complete totals shape, including `activity_count`, while current period totals count distinct work keys. A work item recorded on two days contributes one period activity and two daily memberships. The stated reconciliation rule does not distinguish this nonadditive field.
* **Evidence:** [Wire totals](/workspace/agentsfleet-m249/rustd/crates/daemon/afd_api_wire/src/billing/usage.rs:10) include the count, and [aggregate SQL](/workspace/agentsfleet-m249/rustd/crates/daemon/afd_billing/src/report/sql/aggregates.sql:21) counts distinct `(work_type, work_id)`. [An existing fixture](/workspace/agentsfleet-m249/rustd/crates/daemon/afd_billing/tests/integration_reports.rs:184) gives one work item multiple usage lines. [The ledger schema](/workspace/agentsfleet-m249/schema/20261108124600_billing_usage_lines.sql:41) stores creation time per line.
* **Rationale & Recommendation:** Define daily counts as distinct work within each day, or assign each work item to one specified day. State which totals must add to the period result. Add a fixture with one work key on both sides of midnight.

### Required Rust consumers are outside the declared file scope - Severity: Major

* **IDs:** `implementation_readiness-1-3`, `codebase_details-1-2`
* **Reviewer Lens:** `implementation_readiness`, `codebase_details`
* **Affected Scope:** Target Files Changed, lines 43–54; deferred inventory at line 72.
* **Discrepancy Description:** The listed `library/**` families omit required draft source, publication, creation and OpenAPI owners. The target postpones the concrete inventory until implementation. That leaves mandatory changes outside the declared scope.
* **Evidence:** The omitted owners include:

  | Source | Required change |
  | --- | --- |
  | [Wire source](/workspace/agentsfleet-m249/rustd/crates/daemon/afd_api_wire/src/authoring/input.rs:14) | Rename the serialized discriminator and `library_id` |
  | [Publication receipt](/workspace/agentsfleet-m249/rustd/crates/daemon/afd_api_wire/src/authoring/output.rs:65) | Rename the public receipt field |
  | [Draft decoder](/workspace/agentsfleet-m249/rustd/crates/daemon/afd_api_tenant/src/handler/draft/input.rs:32) | Match the renamed source |
  | [Domain source](/workspace/agentsfleet-m249/rustd/crates/daemon/afd_authoring/src/types/input.rs:13) | Replace opaque entry selection with the required identifier type |
  | [Source capture](/workspace/agentsfleet-m249/rustd/crates/daemon/afd_authoring/src/creation/source.rs:92) | Remove the platform visibility binding |
  | [OpenAPI parameters](/workspace/agentsfleet-m249/rustd/crates/daemon/afd_http/src/openapi/path.rs:256) | Rename parameters and remove platform-slug documentation |

* **Rationale & Recommendation:** Complete the current per-file inventory and record actions before readiness approval. Include these owners, their consumers and registration files. [SPEC-FILES](/workspace/agentsfleet-m249/.orly/standards/spec.md:180) requires the scope to include writers, readers and registration; [SPEC-BLAST](/workspace/agentsfleet-m249/.orly/standards/spec.md:189) requires the rename inventory.

### The metric rename reaches excluded model catalogues - Severity: Major

* **ID:** `codebase_details-1-1`
* **Reviewer Lens:** `codebase_details`
* **Affected Scope:** Target Files Changed, line 49; Metrics & Observability, line 240; exclusions at lines 72 and 257.
* **Discrepancy Description:** The target explicitly renames `metrics/declared/library.rs`. Its signals measure model-catalogue reads, which the same target excludes from the Plugin rename. Renaming them would describe model traffic as Plugin traffic.
* **Evidence:** [The closed metric surfaces](/workspace/agentsfleet-m249/rustd/crates/protocol/af_observability/src/metrics/label/library.rs:12) contain only `TenantModels` and `GlobalModels`. Their callers select those surfaces in [model entries](/workspace/agentsfleet-m249/rustd/crates/daemon/afd_api_tenant/src/handler/tenant/model_entry/mod.rs:184) and [model listing](/workspace/agentsfleet-m249/rustd/crates/daemon/afd_api_tenant/src/handler/tenant/models/mod.rs:107). [The metric declarations](/workspace/agentsfleet-m249/rustd/crates/protocol/af_observability/src/metrics/declared/library.rs:19) carry the affected names.
* **Rationale & Recommendation:** Explicitly preserve this metric family. Rename the separate [publication product event](/workspace/agentsfleet-m249/rustd/crates/protocol/af_observability/src/product/telemetry/authoring.rs:66), which describes the reusable setup being renamed.

### Scope summaries describe an older document - Severity: Nit

* **ID:** `style-1-1`
* **Reviewer Lens:** `style`
* **Affected Scope:** Target Product Clarity, line 271; Decomposition, line 280.
* **Discrepancy Description:** “Four Sections by surface” no longer matches the seven substantive Sections. “What we build” omits the auth, summary and Usage work in §§5–6.
* **Rationale & Recommendation:** Update both summaries to describe the approved scope. Preserve the detailed requirements at target lines 179–216.

### Visual-reference instructions need reproducible links - Severity: Nit

* **ID:** `style-1-2`
* **Reviewer Lens:** `style`
* **Affected Scope:** Target lines 185 and 194.
* **Discrepancy Description:** “The supplied reference,” “the later documented” Profile section and uploaded `proposal.html` require knowledge outside the document.
* **Evidence:** The supplied [proposal](/workspace/artifacts/m249-proposal/visual-review-candidate-ed7c4032b-desktop-a68d0c18f6e545dd94f5ad4161786bd3/proposal.html) is available locally. The [design-system requirement](/workspace/agentsfleet-m249/docs/DESIGN_SYSTEM.md:1136) defines the always-mounted native account section.
* **Rationale & Recommendation:** Link the exact reference and design-system section. These links let a fresh reader identify the intended comparison without searching the conversation.

### Discovery retains a resolved review prerequisite - Severity: Nit

* **ID:** `style-1-3`
* **Reviewer Lens:** `style`
* **Affected Scope:** Target line 287, compared with lines 28 and 288.
* **Discrepancy Description:** Discovery says readiness review awaits the permission decision, while nearby text already records the approval.
* **Rationale & Recommendation:** Update the current review status or label the older statement as historical. Preserve the owner's quoted approval.

### Dense paragraphs obscure separate requirements - Severity: Nit

* **ID:** `style-1-4`
* **Reviewer Lens:** `style`
* **Affected Scope:** Target lines 21, 100 and 175.
* **Discrepancy Description:** These paragraphs contain six, five and five sentences respectively. [DOC-03](/workspace/agentsfleet-m249/.orly/standards/docs.md:78) limits prose paragraphs to three sentences.
* **Rationale & Recommendation:** Split the solution summary, route mapping and source-scan classification into shorter paragraphs. Retain the identity-transition exception and rejection-test exclusions. This editorial finding makes no claim about whether a gate checks historical spec records.

## Read Coverage and Limits

The technical reviews read relevant route, authorization, schema, authoring, command-line, browser, billing and observability sources. They also inspected linked architecture and dependency-spec sections.

The style review read the documentation and spec standards, linked reference sections, production appearance components and the supplied visual proposal.

The review assessed implementation instructions and source grounding. It did not verify runtime behavior or approve deployment, live identity changes or production edits.

The root agent owns resolution and any subsequent review of amended text.

## Targeted Resolution Pass

**Outcome:** All four Major findings and all four Nits above are resolved. No finding from this review remains open; implementation can begin.

The synthesizer checked the amended document and relevant source directly. This was a targeted resolution pass, without another general reviewer round.

**Amended target:** 319 lines; SHA-256 `23538cf316142c4ad2fa40cbc6597c8eac03c33e9a48dce27ee5a4481bdc9b04`.

**Inventory:** SHA-256 `28f48d8336e96237c543befe376206931f02fe172d86443450680720ff59cc8f`.
Read-only `python3` inspection with `csv.DictReader` returned `Rows: 967 Unique paths: 967` for the [inventory](/workspace/agentsfleet-m249/docs/v2/evidence/M249_003_BLAST_INVENTORY.tsv:1).

| Initial finding | Status | Resolution evidence |
| --- | --- | --- |
| `implementation_readiness-1-1` | Resolved | [§1a](/workspace/agentsfleet-m249/docs/v2/pending/M249_003_P1_API_CLI_DOCS_UI_PLUGIN_AND_ARCHIVED_VOCABULARY.md:123) now requires a verified maximum-token-lifetime pause, rejection of the old token, native writer refresh and narrowed-reader checks. [The verifier](/workspace/agentsfleet-m249/rustd/crates/daemon/afd_identity/src/jwks/verifier.rs:127) rejects expiry at the exact boundary without leeway. |
| `implementation_readiness-1-2`, `codebase_details-1-3` | Resolved | [§6](/workspace/agentsfleet-m249/docs/v2/pending/M249_003_P1_API_CLI_DOCS_UI_PLUGIN_AND_ARCHIVED_VOCABULARY.md:220) defines distinct daily work counts as nonadditive and all other totals as additive. [Dimension 6.1](/workspace/agentsfleet-m249/docs/v2/pending/M249_003_P1_API_CLI_DOCS_UI_PLUGIN_AND_ARCHIVED_VOCABULARY.md:235) requires the same work key across midnight. |
| `implementation_readiness-1-3`, `codebase_details-1-2` | Resolved | [Files Changed](/workspace/agentsfleet-m249/docs/v2/pending/M249_003_P1_API_CLI_DOCS_UI_PLUGIN_AND_ARCHIVED_VOCABULARY.md:48) now includes authoring types, creation, draft consumers, wire receipts and OpenAPI owners. [The current inventory](/workspace/agentsfleet-m249/docs/v2/pending/M249_003_P1_API_CLI_DOCS_UI_PLUGIN_AND_ARCHIVED_VOCABULARY.md:79) records per-file roles, meanings and actions, including all previously omitted owners. |
| `codebase_details-1-1` | Resolved | [Observability scope](/workspace/agentsfleet-m249/docs/v2/pending/M249_003_P1_API_CLI_DOCS_UI_PLUGIN_AND_ARCHIVED_VOCABULARY.md:262) preserves model-catalogue metrics. Inventory rows [349](/workspace/agentsfleet-m249/docs/v2/evidence/M249_003_BLAST_INVENTORY.tsv:349), [351](/workspace/agentsfleet-m249/docs/v2/evidence/M249_003_BLAST_INVENTORY.tsv:351), [509](/workspace/agentsfleet-m249/docs/v2/evidence/M249_003_BLAST_INVENTORY.tsv:509) and [515](/workspace/agentsfleet-m249/docs/v2/evidence/M249_003_BLAST_INVENTORY.tsv:515) now retain the model-only callers, declarations and producer. Publication telemetry remains in scope. |
| `style-1-1` | Resolved | [Product Clarity](/workspace/agentsfleet-m249/docs/v2/pending/M249_003_P1_API_CLI_DOCS_UI_PLUGIN_AND_ARCHIVED_VOCABULARY.md:293) includes auth, summary and Usage. [Decomposition](/workspace/agentsfleet-m249/docs/v2/pending/M249_003_P1_API_CLI_DOCS_UI_PLUGIN_AND_ARCHIVED_VOCABULARY.md:302) describes the current scope without the stale Section count. |
| `style-1-2` | Resolved | [§5](/workspace/agentsfleet-m249/docs/v2/pending/M249_003_P1_API_CLI_DOCS_UI_PLUGIN_AND_ARCHIVED_VOCABULARY.md:204) and [§6](/workspace/agentsfleet-m249/docs/v2/pending/M249_003_P1_API_CLI_DOCS_UI_PLUGIN_AND_ARCHIVED_VOCABULARY.md:213) link the available proposal. The design-system anchor resolves to [Models, API Keys, and Team](/workspace/agentsfleet-m249/docs/DESIGN_SYSTEM.md:301), which contains the Profile requirements. |
| `style-1-3` | Resolved | [Discovery](/workspace/agentsfleet-m249/docs/v2/pending/M249_003_P1_API_CLI_DOCS_UI_PLUGIN_AND_ARCHIVED_VOCABULARY.md:309) records the completed review and links this resolution pass. The [earlier readiness record](/workspace/agentsfleet-m249/docs/v2/evidence/M249_003_READINESS_REVIEW.md:7) now records owner approval. |
| `style-1-4` | Resolved | The original [solution summary](/workspace/agentsfleet-m249/docs/v2/pending/M249_003_P1_API_CLI_DOCS_UI_PLUGIN_AND_ARCHIVED_VOCABULARY.md:21), [route mapping](/workspace/agentsfleet-m249/docs/v2/pending/M249_003_P1_API_CLI_DOCS_UI_PLUGIN_AND_ARCHIVED_VOCABULARY.md:109) and [source-scan instructions](/workspace/agentsfleet-m249/docs/v2/pending/M249_003_P1_API_CLI_DOCS_UI_PLUGIN_AND_ARCHIVED_VOCABULARY.md:192) are split. New token and daily-count explanations also have separate paragraphs. |

Provider lifetime verification and before/after authorization checks remain deployment requirements in §1a. This pass did not run tests or change live identities.
