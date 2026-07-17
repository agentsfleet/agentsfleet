# EXECUTE — Doc reads by trigger

> Parent: [`../AGENTS.md`](../AGENTS.md) §EXECUTE. Each dispatch entry's trigger header reads its façade; this table is the canonical trigger→doc map, enforced per edit by the `📖 DOC READ: <path>` proof-line.

Every triggered edit requires a `📖 DOC READ: <path>` proof-line citing §N applied, or the cited-skip variant when nothing in the doc applies. Skipping without a proof-line is a violation regardless of whether the edit happens to be clean.

| Trigger | Read |
|---|---|
| Always (universal) | `docs/greptile-learnings/RULES.md`; re-read on sub-task shape change. |
| `orly/**`, agent-home instruction links, repository rules synchronization, or governance hooks | `dispatch/edit_rules.md` + `docs/ORLY_ARCHITECTURE.md` — canonical sources, profiles, snapshots, refusal boundaries, and evidence. |
| Any source file (cross-cutting authoring) | `dispatch/write_any.md` — length, logging, milestone-id, error-registry, UFS, greptile read, legacy-workaround family. |
| Spec's "Applicable Rules" | Each rule (canonical). Missing → standard set is floor; surface omission. |
| `*.zig` | `dispatch/write_zig.md`. ZIG GATE per edit. |
| `*.ts`/`*.tsx`/`*.js`/`*.jsx` | `dispatch/write_ts_adhere_bun.md` — TS FILE SHAPE DECISION (§1) at PLAN, const/import/Bun-primitive discipline, anti-patterns. |
| `*.rs` | `dispatch/write_rust.md` — ownership, error variants, feature combinations, and contention tests. |
| `*.py` | `dispatch/write_python.md` — parsing boundaries, resource ownership, and specific exceptions. |
| `*.sh` | `dispatch/write_shell.md` — quoting, array arguments, cleanup, input safety, and shell compatibility. |
| `*.mdx` | `dispatch/write_mdx.md` — structure, front matter, links, code fences, accessibility, and Mintlify isolation. |
| Log emit (any language; see LOGGING GATE triggers) | `docs/LOGGING_STANDARD.md` — wire format (logfmt), severity ladder, error-code embedding, scope/event discipline, PII redaction, §10A tightenings. LOGGING GATE per edit. |
| Lifecycle method in `*.zig` (`init|deinit|close|release|destroy|shutdown|dispose|free`) | `docs/LIFECYCLE_PATTERNS.md` — init/deinit pairing, errdefer placement, allocator ownership, defer/errdefer mutual exclusion, §10A tightenings. LIFECYCLE GATE per edit. |
| `src/agentsfleetd/http/handlers/**` or `public/openapi/**` | `docs/REST_API_DESIGN_GUIDELINES.md` — Quick Checklist; §1–§5 (URL/method/body/response/error), §6 (OpenAPI), §7 (6-place route registration), §8 (`Hx` handler interface), §10 (pre-PR gates). |
| `ui/packages/**/*.{tsx,jsx,css}`, `app/**/*.{tsx,jsx,css}`, `components/**/*.{tsx,jsx,css}`, repo-root `globals.css`, or any file changing visual tokens / motion / typography | `DESIGN.md` (repo root) or `docs/DESIGN_SYSTEM.md` — whichever the repo carries. Design system source of truth: typography stack, color tokens, the single accent and its currency rule, motion signature, spacing/density, component principles, CLI palette mapping. DOC READ GATE per edit. |
| `*.tsx` / `*.jsx` under `ui/packages/{app,website}/` | `dispatch/write_ts_adhere_bun.md` (Design Tokens) — token-utility table (text/tracking/leading/max-w/min-w/spacing/motion/radius/color). DESIGN TOKEN GATE fires per edit; audit via project-side `audits/design-tokens.sh`. |
| Auth-flow | `docs/AUTH.md`. |
| Published `*.mdx`, reusable Markdown JSX (MDX) fragment, customer readme, or public OpenAPI prose | `dispatch/write_documentation.md` → `docs/DOCUMENTATION_RULES.md` — establish page, fragment, API, or changelog scope before narrower guides. |
| Changelog `<Update>` / release note (`changelog.mdx`) | First `dispatch/write_documentation.md`, then `dispatch/write_changelog.md` → `docs/CHANGELOG_VOICE.md`; internal-only ⇒ no entry. |
| `schema/*.sql` / migration | `dispatch/write_sql.md` + `docs/SCHEMA_CONVENTIONS.md` — naming/type conventions, schema/migration rules + Schema Table Removal Guard. Re-print Schema Guard output. |
| Any spec under `docs/v*/{pending,active,done}/` or `docs/TEMPLATE.md` | `docs/TEMPLATE.md` "Prohibited" section — no time/effort estimates, no complexity ratings, no percentage-complete, no owners/dates. SPEC TEMPLATE GATE per edit. |
