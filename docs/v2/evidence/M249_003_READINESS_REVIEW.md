# M249_003 readiness review

Date: Nov 13, 2026 (UTC). Base: `646fc536e`.
M249_001 merged in `646fc536e` (PR #840); M249_002 merged in `baa26abed` (PR #839).

## Verdict

Indy approved the exact identity-provider grant replacement. The uploaded proposal is available, and the deleted development draft is excluded at his request.
The final readiness findings and their resolution are recorded in `M249_003_FINAL_READINESS.md`; backend implementation follows that check.

## Independent reviews

Fresh-context reviewers read their Elephant-Goldfish Model personas and applicable standards, the specification and relevant source.
The initial zoom-out, comprehension, design, structural and style passes completed.
The intermediate zoom-out found no new architectural or scope issue after the mitigations below.

The final readiness pass identified browser-token renewal, daily activity-count semantics, missing source owners and excluded model metrics.
The amended spec specifies these behaviors and links the current per-file inventory.

| Finding | Evidence | Disposition |
| --- | --- | --- |
| Existing grants use the removed permission spelling | `docs/AUTH.md`, Provisioning grants; `afd_auth/src/scope.rs`, `signup_owner_claim`; `afd_identity/src/capability.rs` | Resolved: owner approved exact token replacement, preserving narrowed permissions. §1a requires empty resolver caches and expiry/refresh of existing browser tokens. |
| Browser navigation still serves `/library` | `components/layout/sidebar-navigation-data.ts`; workspace `library/page.tsx` | Spec now includes the route move and an authenticated not-found proof. |
| Source scan rejects its own negative tests | Spec Dimensions 2.1 and 4.2 | Scan now distinguishes active callers, explicit rejection fixtures and historical records. |
| Broad rename reaches unrelated model-library types | `lib/api/model-library-types.ts` | Explicitly excluded by meaning. |
| Remaining ownership and visibility are unspecified | Entries schema; authoring `library/sql/entry.sql` and `creation/sql/library.sql` | Spec now requires account ownership on every access path and removes the constant visibility discriminator. |
| Plugin identifier grammar is ambiguous | API resources; wire and authoring lookup types | Spec now requires UUIDv7 for Plugin and revision selections. |
| Dependent draft foreign keys and migration registry are absent from scope | `schema/20261108123600_authoring_drafts.sql`; `afd_db/src/migration.rs` | Added to the declared edit scope. |
| Schema names and deletion semantics are implicit | Entries/revisions schema and draft pointers | Exact table/column mapping, append-only enforcement and both `ON DELETE SET NULL` references now specified. |
| Revision listing and detail output are conflated | `afd_api_wire/src/library/output.rs` | Separate summary columns, nested detail fields and a usable continuation command now specified. |
| Conflict and removed-route proofs do not cover their stated failure modes | Spec Failure Modes table | Added explicit publication conflict/revocation and removed workspace-route checks. |

## Comprehension result

### What this proposes

Rename reusable fleet setups to Plugins across API, permissions, schema, CLI, browser URLs, telemetry and current documentation.
Remove the platform catalogue and redundant owned collection, while retaining immutable revisions, independent fleet copies and existing access boundaries.
Fleet archive/delete behavior belongs to the merged M249_002; capability composition and new sharing models remain excluded.

### How the existing system works, per this document

Reusable setups have immutable revisions and publication checks; saved fleets keep their own copies.
Current read paths permit tenant-owned and platform entries, and the CLI already displays Plugin names while calling the existing API paths.
Person capabilities come from Clerk metadata, so changing code or resetting Postgres cannot rename existing grants.

## Separate owner-requested interface work

The Nov 13 request also authorizes auth theme/copy updates and reviews of Usage, Security, Team and the linked fleet setup.
Those requests extend beyond M249_003's original adapter-only UI scope.
Interface repairs do not establish that the Plugin rename is implemented or ready to merge.

Copy references inspected: [Capy](https://capy.ai/), [Grok Bot](https://x.ai/news/introducing-grok-bot), and [OpenBots](https://github.com/abcampbel/open-bots).
The revised copy describes tasks, connected tools and reviewable results without copying product claims or promising unsupported autonomy.

### Implemented locally

- Shared Clerk appearance now uses the design system's surfaces, 10-pixel control radius, control heights and keyboard focus outline.
- Sign-in/sign-up copy now describes tasks, connected tools and reviewable results.
- Phone inputs retain 16-pixel text to prevent browser zoom; desktop controls use the compact shared scale.
- Setup summary, access and success-criteria text now use the shared 13-pixel scale and wrap long content within the summary.

Password recovery uses the same native SignIn appearance, including verification-code fields.
No recovery email or completed password-reset flow was exercised; that remains an authenticated acceptance check.

### Settings review

Team, API Keys/Security and Usage were rendered from their production components with explicitly isolated sample data.
The desktop/phone matrix covered dark and light themes, with no page errors or horizontal document overflow in 16 renders.
The Profile action reached its always-mounted section; the fixture correctly required authentication for the provider's forms.

Session error and recovery states also rendered correctly. These results do not prove authenticated provider mutations.

Profile is intentional on API Keys, per `docs/DESIGN_SYSTEM.md`, "API Keys combines API keys, signed-in browser sessions and Profile and sign-in settings."
No Team or Security data behavior was changed.

Usage's daily-spending card remains unavailable because its current response has no whole-period daily projection.
M249_004 §2 originally owned the missing data. The owner-requested follow-up now moves that Usage slice into M249_003 §6.

The reference has been rendered at desktop and phone widths in both themes under `/workspace/artifacts/m249-reference/`.

### Local validation

- `make dry-app`: 478 unit files, 4,563 unit tests and 90 browser tests passed before the final control-height and list-scale corrections.
- The focused auth/setup browser suites then passed all 18 checks with those corrections, including native field height and the rendered access-list scale.
- `orly gate work`: passed (`repo.config: 4 command(s) declared`).
- `orly gate verify`: passed repository configuration, documentation language, `make lint-all` and `make check-version` (0.77.0).
- `git diff --check`: passed.
- Gitleaks scanned tracked changes plus new source/doc files: no leaks found.
- Sign-in/sign-up screenshots cover 1,480- and 390-pixel widths, dark and light themes: eight renders, no horizontal overflow.
- Rendered auth buttons measured 40 pixels on desktop and 44 on phone; fields measured 44 pixels at both widths.
- Input text measured 13 pixels on desktop and 16 on phone.

The standards review found copied radius values in browser assertions and a relative import that climbed two directories.
The assertions now resolve the production radius token and use the configured import alias.
The independent reviewer confirmed both fixes and reported no findings against 114 applicable rules.

The full lint gate needed CMake installed under `/workspace/.cloud-tools/m249-cmake` and the existing compiler's standard include directory supplied to Bindgen.
The successful invocation used `PATH=/workspace/.cloud-tools/m249-cmake/cmake/data/bin:$PATH` and `BINDGEN_EXTRA_CLANG_ARGS='-isystem /usr/lib/gcc/x86_64-linux-gnu/14/include'`.
Cargo used two jobs, disabled incremental compilation and disabled development/test debug information, as required by `.orly/LOCAL.md`.

Screenshots and measurements are in `/workspace/artifacts/m249-followup/` for auth, settings, profile and summary.
Summary screenshots use explicitly isolated sample requirements, including network access and a daily spending limit.
They demonstrate the rendered layout and do not reproduce the reported live failure.

Changes remain local and uncommitted on `feat/m249-plugin-vocabulary-ui-followup` in `/workspace/agentsfleet-m249`.
No pull request, deployment, backend rename, database reset or live identity change has been performed.

## Live-review disposition

The uploaded archive is extracted under `/workspace/artifacts/m249-proposal/` and its Usage view was rendered in a fresh browser context.
The Vercel bypass reached the application's normal sign-in screen. No authenticated fleet data was inspected or changed.

Indy then stated that the database was torn down and the URL was invalid.
He added: "i will give you a nother one later" and "so move on to the next".

The old URL is no longer a review target. Its reported failure remains unverified until a replacement is provided.

## Grant transition fixture proof

The one-off `/tmp/m249-grant-transition.mjs` helper passed eight transformation fixtures and six malformed-input checks.
Those cover exact-token replacement, platform-grant removal, narrowed readers, missing claims, unrelated metadata, substring exclusions and idempotency.
No live identities were changed; the deployment verification in §1a remains due when the renamed deployment is prepared.
