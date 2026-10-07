# EXECUTE — Doc reads by trigger

> Parent: [`../AGENTS.md`](../AGENTS.md) §EXECUTE. Each dispatch entry's trigger header reads its façade — sectioned: scan headers, read the sections the diff shape touches; this table is the canonical trigger→doc map, enforced by the `📖 DOC READ: <path>` proof-line.

Every triggered document requires one `📖 DOC READ: <path>` proof-line **per turn** — before the turn's first triggering edit — citing §N applied, or the cited-skip variant when nothing in the doc applies. Auto-mode and prior-turn reads never excuse the line (already-loaded sections are cited, not re-read); a triggering turn without the line is a violation regardless of whether the edits happen to be clean.

| Trigger | Read |
|---|---|
| Always (universal) | `.orly/docs/greptile-learnings/RULES.md` — at EXECUTE start read the Rule-code gloss legend + the sections named by the spec's "Applicable Rules" list (canonical); the full file belongs to `/review`, not EXECUTE. On sub-task shape change, section-scan (`grep -n "^## "`) and read only newly-touched sections, cited in the proof-line — never the full file again. |
| Lifecycle stage transition (CHORE(open)/PLAN/CHORE(close)/LAND, worktree setup, milestone bootstrap) | `.orly/dispatch/lifecycle.md` — the entered stage's runbook section only (section-scan; fires on the transition itself, not on an Edit/Write). |
| Any source file (cross-cutting authoring) | `.orly/dispatch/write_any.md` — length, logging, milestone-id, error-registry, UFS, greptile read, legacy-workaround family. |
| Porting a codebase, module, or subsystem from another language | `.orly/dispatch/write_any.md` §Porting a codebase between languages — a port is a rewrite with a conformance test: port the guarantee, delete the workaround, conform on observable behaviour, name the debt not carried over. |
| Spec's "Applicable Rules" | Each rule (canonical). Missing → the cross-cutting codes of `.orly/dispatch/write_any.md` are the floor; surface omission. |
| `*.zig` | `.orly/dispatch/write_zig.md`. ZIG GATE per edit. |
| `*.ts`/`*.tsx`/`*.js`/`*.jsx` | `.orly/dispatch/write_ts_adhere_bun.md` — TS FILE SHAPE DECISION (§1) at PLAN, const/import/Bun-primitive discipline, anti-patterns. |
| `*.rs` | `.orly/dispatch/write_rust.md` — ownership, error variants, feature combinations, and contention tests. |
| Rust error type, `Result` alias, or any fallible signature | `.orly/dispatch/write_rust.md` §Error discipline — one error per crate, one `Result` alias, `#[from]` composition, `source()` never returns self. ERR-RS fires per edit: `.orly/audits/rust-error.sh` (via `.orly/dispatch/write_rust.sh`) blocks a `map_err` that stringifies its own cause and an error type with no alias in its crate; the remaining clauses stay judgment. |
| Designing a Rust function, type, or module; writing `match` on a `Result`/`Option`; any production `unwrap()` | `.orly/dispatch/write_rust.md` §Functional design — Result pipelines over control-flow trees, illegal states unrepresentable, bind once, `unwrap` needs a proved invariant. |
| `*.py` | `.orly/dispatch/write_python.md` — parsing boundaries, resource ownership, and specific exceptions. |
| `*.sh` | `.orly/dispatch/write_shell.md` — quoting, array arguments, cleanup, input safety, and shell compatibility. |
| Log emit (any language; see LOGGING GATE triggers) | `.orly/docs/LOGGING_STANDARD.md` — wire format (logfmt), severity ladder, error-code embedding, scope/event discipline, PII redaction, §10A tightenings. LOGGING GATE per edit. |
| Lifecycle method in Zig | `.orly/docs/LIFECYCLE_PATTERNS.md` — ownership, acquisition cleanup, repeated cleanup of surviving values and exactly-once release of self-destroying objects. |
| HTTP handlers, route registration or OpenAPI | `.orly/docs/REST_API_DESIGN_GUIDELINES.md` — endpoint checklist and the repository's mounted authorization and handler shape. |
| `ui/packages/**/*.{tsx,jsx,css}`, `app/**/*.{tsx,jsx,css}`, `components/**/*.{tsx,jsx,css}`, repo-root `globals.css`, or any file changing visual tokens / motion / typography | `DESIGN.md` (repo root) or `docs/DESIGN_SYSTEM.md` — whichever the repo carries. Design system source of truth: typography stack, color tokens, the single accent and its currency rule, motion signature, spacing/density, component principles, CLI palette mapping. DOC READ proof-line per turn. |
| `*.tsx` / `*.jsx` under `ui/packages/{app,website}/` | `.orly/dispatch/write_ts_adhere_bun.md` (Design Tokens) — token-utility table (text/tracking/leading/max-w/min-w/spacing/motion/radius/color). DESIGN TOKEN GATE fires per edit; audit via project-side `.orly/audits/design-tokens.sh`. |
| Auth-flow | `docs/AUTH.md`. |
| Published `*.mdx`, reusable Markdown JSX (MDX) fragment, customer readme, or public OpenAPI prose | `.orly/dispatch/write_documentation.md` → `.orly/docs/DOCUMENTATION_RULES.md` — establish page, fragment, API, or changelog scope before narrower guides. |
| Changelog `<Update>` / release note (`changelog.mdx`) | First `.orly/dispatch/write_documentation.md`, then `.orly/dispatch/write_changelog.md` → `.orly/docs/CHANGELOG_VOICE.md`; internal-only ⇒ no entry. |
| Write or update a PR/MR body, including session notes (`gh pr create/edit --body`, `glab mr create/update --description`, forge API, helper, or skill) | `.orly/dispatch/write_pr_description.md` — append-only numbered session unit, measured diagram, review findings, and command output. |
| SQL / migration registration | `.orly/dispatch/write_sql.md` + `.orly/docs/SCHEMA_CONVENTIONS.md` — the repository's live-data policy and selected product guard. |
| Any spec under `docs/v*/{pending,active,done}/` or `.orly/docs/TEMPLATE.md` | `.orly/docs/TEMPLATE.md` "Prohibited" section — no time/effort estimates, no complexity ratings, no percentage-complete, no owners/dates. SPEC TEMPLATE GATE per edit. |
