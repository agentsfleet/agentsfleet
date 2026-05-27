<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates anywhere in this spec.
- No effort columns, complexity ratings, percentage-complete, implementation dates.
- No assigned owners — use git history and handoff notes.
- Priority (P0/P1/P2) is the only sizing signal. Use Dependencies for sequencing.
- If a section below contradicts these rules, the rule wins — delete the section.
- Enforced by SPEC TEMPLATE GATE (`docs/gates/spec-template.md`) and `scripts/audit-spec-template.sh`.
- See docs/TEMPLATE.md "Prohibited" section above for canonical list.
-->

# M75_001: `usezombie.sh` — one-URL installer (POSIX; PowerShell follow-up)

**Prototype:** v2.0.0
**Milestone:** M75
**Workstream:** 001
**Date:** May 18, 2026
**Status:** DONE
**Priority:** P2 — not blocking any other workstream, but the broken curl path is an onboarding hazard (every doc page that names `usezombie.sh/skills.md` today serves a DNS failure to readers). Captain decision May 18, 2026 to make `usezombie.sh` the canonical entrypoint, replacing the M51/M72 plan that had it serving per-skill `*.md` files.
**Categories:** CLI, DOCS, INFRA
**Batch:** B1
**Branch:** feat/m75-001-usezombie-sh-install-domain
**Depends on:** None — greenfield install path. M71_001 (DONE) and M72_001 (DONE) shipped the docs that today reference the broken `usezombie.sh/skills.md`; this spec fixes those references as part of §3.
**Provenance:** agent-generated. Captain in-session decision on May 18, 2026 after PR #330 (M71_001 P2) cycle surfaced that `usezombie.sh/skills.md` is referenced in four user-facing docs but the domain doesn't resolve (DNS NXDOMAIN). Captain settled the intent as "`https://usezombie.sh` should start the install of zombiectl AND `npx skills add`" — both, one URL, classic one-liner installer.

**Canonical architecture:** `docs/architecture/user_flow.md` §8 (cold-machine bootstrap) — already names `https://usezombie.sh` as the host-skill install entry; this spec implements that contract on disk and wires it to the live domain.

---

## Scope amendments (May 21, 2026 — during implementation)

Four Captain-acked changes from the original plan. Reality and Indy decisions trump the spec; sections below were trimmed to match.

1. **npm-only install — no tarball fallback.** `zombiectl` is an npm-distributed Node CLI (`bin: ./dist/bin/zombiectl.js`); there is no standalone binary and no `zombiectl-{platform}-{arch}` release tarball (GitHub Releases ship the `zombied-*` daemon, not the CLI). Node is a hard prerequisite — missing `node`/`npm` exits 5 with an "install Node.js ≥18" message. §1, the exit-code table, Failure Modes, and the Test Spec were amended accordingly.
2. **Live DNS/TLS cutover deferred.**
   > Indy (2026-05-21): chose "Author config, defer live cutover" — this PR ships scripts + tests + Cloudflare Pages config + README in-repo; live DNS provisioning + the live-only acceptance criteria (DNS A/AAAA, TLS, containerized E2E) are deferred to a post-merge cutover.

   Stale npm (`0.3.0`) / GH-release (`v0.4.0`) artifacts vs repo `0.37.0` are out of scope here (a CLI republish is a separate release task).
3. **PowerShell installer deferred to a follow-up spec/PR.**
   > Indy (2026-05-21): chose "Drop PowerShell from this PR" — `install.ps1` + `install_test.ps1` + the Windows test / exit-6 / PS-floor rows leave M75 scope; ship POSIX-only. Docs show only the `curl … | bash` one-liner until the follow-up lands.

   The Windows installer becomes its own spec (authored via `kishore-spec-new` when that work begins).
4. **Skill ref is `usezombie/skills`, not `usezombie/usezombie`.**
   > Indy (2026-05-21): the canonical skill source is the public skills repo `usezombie/skills` (populated by M69 `PUBLIC_SKILLS_REPO`, merging the same day). `install.sh` (`SKILL_REF`), its tests, the README, and the docs sweep all reference `usezombie/skills` so the `curl … | bash` path and the documented `npx skills add` path pull from the same repo.

---

## Implementing agent — read these first

1. `~/Projects/oss/resend-cli/install.sh` + `~/Projects/oss/resend-cli/install.ps1` — closest pattern reference. Two scripts (POSIX bash + Windows PowerShell) served at two URLs from the same domain (`resend.com/install.sh` and `resend.com/install.ps1`). The `install.sh` wraps in a `main()` function for partial-download protection, colored output gated on `[[ -t 1 ]]`, GitHub-release tarball fetch with a `RESEND_VERSION` pin, install-dir env override, version-pin via `bash -s vX.Y.Z`. Mirror this shape verbatim — substitute zombiectl-specific bits.
2. `~/Projects/oss/bun/oss/bun/src/cli/install/bash_installer.zig` (or whatever the Bun installer template is — the implementing agent verifies the actual file) — Bun's `bun.sh/install` + `bun.sh/install.ps1` is the largest-scale public example; it informs the architecture decision (separate URLs vs UA-sniffed content negotiation). Convention is **two URLs**, no content negotiation — every adjacent reference (resend, deno, starship, rustup) follows it.
3. `docs/architecture/user_flow.md` §8 — already declares `https://usezombie.sh` as the canonical bootstrap entry for humans. This spec implements that declaration; do not invent a new URL or path.
4. `~/Projects/docs/changelog.mdx` line ~44 (May 17, 2026 entry), `~/Projects/docs/quickstart.mdx:74`, `~/Projects/docs/cli/install.mdx:48`, `~/Projects/docs/zombies/install.mdx:43` — the four broken `curl -fsSL https://usezombie.sh/skills.md \` references this spec retires. §3 sweep is required, not opportunistic.
5. `docs/v2/done/M51_001_P1_DOCS_SITE_REWRITE_AND_ARCH_CROSSREF.md` §"Out of Scope" + `M72_001` — these specs banned `usezombie.sh/install.sh` from `marketing-spec.test.ts`'s banned-strings list. M75 deliberately re-enables it. The banned-strings test is the regression gate; updating it is mandatory, not opportunistic.

---

## PR Intent & comprehension handshake

> The bridge from spec to merged PR — the agent confirms intent before writing code.

- **PR title (eventual):** usezombie.sh: one-URL POSIX installer for zombiectl + platform-ops skill
- **Intent (one sentence):** `https://usezombie.sh | bash` installs `zombiectl` + the platform-ops skill in one documented command on Linux/macOS, retiring the broken `usezombie.sh/skills.md` curl path; live DNS/TLS and the Windows PowerShell installer are deferred (Scope-amendments 2–3).
- **Handshake (agent fills at PLAN, before EXECUTE):** restate the intent in your own words and list the assumptions you proceed on (`ASSUMPTIONS I'M MAKING: …`). Three to name: (1) the canonical one-liner is the bare-root form `curl -fsSL https://usezombie.sh | bash` (no `/install` path); (2) DNS/TLS lands out-of-tree and must be verified live before merge; (3) the M51 `usezombie.sh/install.sh` banned-strings entry is deliberately removed, not worked around. A mismatch with the Intent above → STOP and reconcile before any edit.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — universal repo discipline. RULE NDC (no dead code), RULE NLR (touch-it-fix-it), RULE UFS (unique-functionality strings — the install one-liner is shared between the script comments, both docs sites, and the `marketing-spec.test.ts` positive assertion; pin once).
- **`docs/greptile-learnings/RULES.md` RULE NLG** — no "legacy" framing for the M51 banned-strings entry being removed. The old `usezombie.sh/skills.md` plan was superseded, not deprecated; remove the entry cleanly without legacy-shim prose.
- **`docs/REST_API_DESIGN_GUIDELINES.md`** — N/A; this spec adds no HTTP handlers on the usezombie service.
- **`docs/SCHEMA_CONVENTIONS.md`** — N/A; no schema changes.
- **`docs/ZIG_RULES.md`** — N/A; `ui/usezombie.sh/dist/install.sh` is bash, no Zig touched.
- **`docs/AUTH.md`** — N/A; install path is unauthenticated. Authenticated install variants are explicitly Out of Scope.
- Shellcheck — `install.sh` (and `install_test.sh`) must pass `shellcheck -s bash` with zero warnings. (The PowerShell `Invoke-ScriptAnalyzer` gate moves to the follow-up spec.)

---

## Applicable Gates

> Which Action-Triggered Guards this PR trips, and how each stays clean. Blast radius: `ui/usezombie.sh/{dist/install.sh,dist/_redirects,dist/_headers,install_test.sh,README.md}`, `.github/workflows/lint.yml` (one new `lint-usezombie-sh` job), `playbooks/014_usezombie_sh_deploy/`, cross-repo docs `.mdx`, and two `.ts` marketing-spec tests (+ one `.tsx` fixture). No Zig, no schema, no PowerShell (deferred).

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | no | `install.sh` is bash — no Zig touched. |
| PUB / Struct-Shape | no | no Zig surface. |
| File & Function Length (≤350/≤50/≤70) | yes | `.sh` is in the length surface — use a `main()` wrapper + small helpers; keep each script ≤350 lines and shell functions ≤50; split if the script approaches the cap. |
| UFS (repeated/semantic literals) | yes | the install one-liner is shared across script comments, both docs sites, and the `marketing-spec.test.ts` positive assertion — pin once and reference verbatim (RULE UFS). |
| UI Substitution / DESIGN TOKEN | no | `InstallBlock.test.tsx` is a fixture-string verify, not a component-markup change. |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | LOGGING: yes | `.sh` is in the LOGGING surface; script output is user-facing install progress (not the app's structured logger) — no secrets echoed, errors actionable and to stderr, color gated on `[[ -t 1 ]]`. ERROR REGISTRY/LIFECYCLE/SCHEMA: no (numeric exit codes 0–5, no `UZ-*` codes, no Zig lifecycle, no schema). |
| `check-gh-actions-valid` (CI/CD edit) | yes | `lint.yml` gains a `lint-usezombie-sh` job (shellcheck + `install_test.sh`) that gates the merge — actionlint clean. No deploy workflow: `usezombie.sh` is a git-connected Cloudflare Pages project (Captain direction, May 21, 2026), auto preview-on-PR + prod-on-merge, no CI credentials. |

---

## Overview

**Goal (testable):** a user on Linux or macOS can run a single documented one-liner (`curl -fsSL https://usezombie.sh | bash`) that installs `zombiectl` via npm, puts it on PATH, runs `npx skills add usezombie/skills --host=<detected>`, and prints a "next command" hint. The script fails fast with an actionable error on any of: missing Node toolchain, network unreachable, npm install failure, install-dir not writable, host detection ambiguous. (Windows PowerShell + live DNS/TLS are deferred — Scope-amendments 2–3.)

**Problem:** Four user-facing doc pages today print `curl -fsSL https://usezombie.sh/skills.md > <path>/SKILL.md` as the curl-fallback install. `usezombie.sh` does not resolve — DNS NXDOMAIN. Every reader who follows the docs literally sees `curl: (6) Could not resolve host: usezombie.sh`. The skill-per-MD path was the M51 plan; Captain superseded it on May 18, 2026 with the one-URL one-liner pattern.

**Solution summary:** Author the POSIX `install.sh` (+ `install_test.sh`) as a static site under `ui/usezombie.sh/` (`dist/` committed + served as-is, no build), with `_redirects`/`_headers` to serve it at `/` and `/install.sh`. Deploy via a **git-connected Cloudflare Pages project** (same pattern as the other `ui/` sites — auto preview-on-PR + prod-on-merge, no workflow, no CI creds); a new `lint-usezombie-sh` CI job (shellcheck + `install_test.sh`) gates the merge. One-time provisioning is `playbooks/014_usezombie_sh_deploy/` (live provisioning deferred). Sweep the four docs to print the canonical POSIX one-liner. Remove `usezombie.sh/install.sh` from the `marketing-spec.test.ts` banned-strings list and add a positive assertion that the canonical one-liner appears in at least one user-facing doc page. Update the `InstallBlock.tsx` fixture to the bare-root one-liner. The Windows PowerShell installer is a deferred follow-up (Scope-amendment 3).

---

## Prior-Art / Reference Implementations

> Mirror a known-good installer shape — don't invent a bootstrap pattern.

- **Installer scripts** → `~/Projects/oss/resend-cli/install.sh` + `install.ps1` (Implementing-agent read #1): two scripts at two URLs from the same domain, `main()` wrapper for partial-download safety, color gated on `[[ -t 1 ]]`, GitHub-release tarball with a version pin, install-dir env override. Mirror verbatim. Largest-scale example: bun's `bun.sh/install` + `bun.sh/install.ps1`.
- **CLI DX** → `docs/CLI_DX_PILLARS.md`. The 7 Pillars target the `zombiectl` CLI itself; the installer is *pre-zombiectl bootstrap*, so they apply loosely — the relevant ones are actionable structured errors (the numeric exit-code table) and clear human-vs-machine output.
- **Alignment:** the two-URL, no-content-negotiation convention is universal across resend / bun / deno / starship / rustup. No divergence.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `ui/usezombie.sh/dist/install.sh` | NEW | POSIX installer. Top-level helpers + thin `main()` (partial-download safe — only the final `main "$@"` has side effects); detects host (`claude`, `amp`, `codex`, `opencode`, generic); installs `zombiectl` via `npm install -g --prefix` (Node a hard prereq, no tarball fallback); runs `npx --yes skills add usezombie/skills --host=<detected>`; sets PATH; prints success + next-command hint. |
| `ui/usezombie.sh/dist/_redirects` + `ui/usezombie.sh/dist/_headers` | NEW | Cloudflare Pages static-host config (committed, served as-is — no build): serve `install.sh` at `/` and `/install.sh`; shell content-type + 5-min cache. |
| `.gitignore` | EDIT | The repo-wide `dist/` ignore swallowed `ui/usezombie.sh/dist/` (the committed static site). Replace it with build-output-anchored rules (`zombiectl/dist/`, `ui/packages/*/dist/`) so the installer payload is tracked while real build artifacts stay ignored. |
| `ui/usezombie.sh/install_test.sh` | NEW | Black-box smoke test for `dist/install.sh` — hermetic fake `npm`/`npx`/`node`/host-bins on a sandboxed `$PATH`, asserts argv + exit codes per failure mode + the next-command hint. |
| `ui/usezombie.sh/README.md` | NEW | One-page contributor doc — what the installer does, the `dist/` layout, how to test locally, how to deploy (git-connected Cloudflare Pages — no workflow). |
| `.github/workflows/lint.yml` | EDIT | Add a `lint-usezombie-sh` job (shellcheck `dist/install.sh` + `install_test.sh`) to the PR lint umbrella — gates the merge so a broken installer can't reach `main` (and therefore can't auto-deploy). |
| `playbooks/014_usezombie_sh_deploy/001_playbook.md` | NEW | One-time human-run setup: create the git-connected `usezombie-sh` Cloudflare Pages project (output `ui/usezombie.sh/dist`, no build), attach `usezombie.sh` custom domain (→ apex A/AAAA + TLS), verify. No CI credentials (Cloudflare's GitHub App auths). |
| `ui/usezombie.sh/dist/install.ps1` + `install_test.ps1` | DEFERRED | Windows PowerShell installer — out of M75 scope (Scope-amendment 3); ships in a PowerShell follow-up spec. |
| `~/Projects/docs/changelog.mdx` | EDIT | Replace the May 17, 2026 entry's `usezombie.sh/skills.md` mention (line ~44) with the new one-liner. Don't rewrite history for prior entries — only touch the line that's actively wrong. |
| `~/Projects/docs/quickstart.mdx` | EDIT | Replace `curl -fsSL https://usezombie.sh/skills.md \` block with the POSIX one-liner `curl -fsSL https://usezombie.sh | bash`. Keep `npx skills add usezombie/skills` as a parallel option for users who already have a node toolchain. |
| `~/Projects/docs/cli/install.mdx` | EDIT | Same swap. |
| `~/Projects/docs/zombies/install.mdx` | EDIT | Same swap. |
| `docs/architecture/user_flow.md` | EDIT | The text naming `https://usezombie.sh/skills.md` as the fetch path is superseded — reconcile to the bare-root `curl -fsSL https://usezombie.sh | bash` one-liner. |
| `ui/packages/website/src/marketing-spec.test.ts` | EDIT | Remove `usezombie.sh/install.sh` from the banned-strings list (was banned per M51 when the plan was different). Add a positive assertion: at least one user-facing doc page contains the canonical one-liner. |
| `ui/packages/website/src/marketing-no-pr-validator-framing.test.ts` | EDIT | Mirror the banned-strings update; this file shares the dead-string list. |
| `ui/packages/design-system/src/design-system/InstallBlock.test.tsx` | VERIFY (no edit expected) | Fixture at line 7 already references `https://usezombie.sh/install | bash`. Verify the new install URL the spec settles on matches verbatim, or update the fixture in this PR. |

> **Anti-pattern guard:** no file in `src/` (Zig), `zombiectl/` (TypeScript CLI), or `ui/packages/app/` is touched by this spec. The install logic lives in the `install.sh` bash script; the dashboard surface is untouched.

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape (POSIX-only this PR):** five slices — POSIX installer + its test, Cloudflare Pages static-host config, docs sweep, marketing-spec banned-string flip, `InstallBlock` fixture. The PowerShell installer is a deferred sixth slice in a follow-up spec (Scope-amendment 3). Two-URL convention preserved, no content negotiation.
- **Alternatives considered:** (a) a single script served at one URL with User-Agent content negotiation — rejected; fragile, and no adjacent project does it. (b) keep the per-skill `*.md` curl plan from M51/M72 — superseded by Indy's one-URL decision (May 18, 2026).
- **Patch-vs-refactor verdict:** **neither a patch nor a refactor of existing code** — a greenfield install path. The only edits to existing files are the docs sweep and the banned-strings flip (cleanup of now-wrong references), kept tight rather than bundled with unrelated changes.

---

## Sections (implementation slices)

### §1 — POSIX installer (`install.sh`)

Bash script that bootstraps `zombiectl` + the platform-ops skill on Linux + macOS. **Implementation default**: mirror the resend-cli `install.sh` shape — `main() { ... }` wrapper for partial-download protection, `set -euo pipefail`, colored output gated on `[[ -t 1 ]]`, install-dir defaults to `~/.usezombie` with `USEZOMBIE_INSTALL` env override, version pin via `bash -s vX.Y.Z`. Host detection inspects `$PATH` for `claude`, `amp`, `codex`, `opencode` binaries in that order, honouring a `USEZOMBIE_HOST` override; falls back to `generic` if none found. **Install mechanism — npm-only (amended May 21, 2026).** `zombiectl` is an npm-distributed Node CLI (`bin: ./dist/bin/zombiectl.js`); there is no standalone binary and no `zombiectl-{platform}-{arch}` release tarball (GitHub Releases ship the compiled `zombied-*` daemon, not the CLI). The CLI and `npx skills add` both require Node, so **Node is a hard prerequisite**: the script runs `npm install -g --prefix "${USEZOMBIE_INSTALL}" @usezombie/zombiectl[@version]`, adds `${USEZOMBIE_INSTALL}/bin` to PATH, then runs `npx skills add usezombie/skills --host=<detected>`. If `node`/`npm` is absent the script exits 5 with an actionable "install Node.js ≥18 from nodejs.org" message — there is no tarball fallback (it could not run a `.js` CLI anyway). Print a "next command" hint that depends on detected host (`/usezombie-install-platform-ops` for Claude Code and the other slash-command hosts).

### §2 — Windows installer (`install.ps1`) — DEFERRED

Out of M75 scope per Scope-amendment 3. The PowerShell installer (`install.ps1` + `install_test.ps1`, the PS-floor exit-6 path, and the Windows test rows) ships in a separate follow-up spec. POSIX-only this PR.

### §3 — Deployable site + git-connected deploy + provisioning playbook

The installer is a **static site under `ui/usezombie.sh/`** (Captain decision, May 21, 2026 — no npm-style "build", `dist/` is committed and served as-is):

- `ui/usezombie.sh/dist/install.sh` — the script (single source of truth).
- `ui/usezombie.sh/dist/_redirects` — `/ → /install.sh 200` so the bare-root `curl … | bash` works.
- `ui/usezombie.sh/dist/_headers` — shell content-type + 5-min cache.
- `ui/usezombie.sh/{install_test.sh,README.md}` — not served.

`usezombie.sh` serves the installer at the apex (bare-root) — it does **not** forward to `usezombie.com/agents` (that prior approach is replaced). Browser visitors get the script; the canonical contract is `curl -fsSL https://usezombie.sh | bash`.

Deploy uses a **git-connected Cloudflare Pages project** — the same pattern as `usezombie-website`/`usezombie-app` (verified: there is no `wrangler` config or `pages deploy` workflow anywhere in the repo; Cloudflare's GitHub App builds + deploys those projects). Cloudflare auto-deploys a **preview** on every PR (and comments the URL) and **production** on merge to `main` → `usezombie.sh`. No deploy workflow, no `wrangler`, no Cloudflare credentials in CI. The merge is gated by a new `lint-usezombie-sh` job in `lint.yml` (shellcheck `dist/install.sh` + run `install_test.sh`), so a broken installer never reaches `main` and therefore never auto-deploys.

The one-time Cloudflare provisioning (Pages project, `usezombie.sh` custom domain → apex A/AAAA + TLS, vault item) lives in `playbooks/014_usezombie_sh_deploy/001_playbook.md`. Live provisioning is the deferred post-merge cutover (Scope-amendment 2). The `/install.ps1` route is added by the PowerShell follow-up spec.

### §4 — Docs sweep (the four broken pages)

Replace every `curl -fsSL https://usezombie.sh/skills.md \` block in `~/Projects/docs/{changelog,quickstart,cli/install,zombies/install}.mdx` with the canonical POSIX one-liner:

- POSIX (Linux/macOS/WSL): `curl -fsSL https://usezombie.sh | bash`

The `npx skills add usezombie/skills` line stays as the "I already have a node toolchain" parallel option — it's not removed, it's positioned alongside the curl one-liner. The Windows PowerShell one-liner (`irm https://usezombie.sh/install.ps1 | iex`) is added by the PowerShell follow-up spec (Scope-amendment 3), not here. Run the existing `~/Projects/docs/` build (`mintlify dev` or equivalent) and verify no MDX parse errors.

### §5 — `marketing-spec.test.ts` banned-string flip

Two test files share the obsolete ban; the M51 ban is removed cleanly (RULE NDC — no comment carries M51 framing forward):

1. `marketing-no-pr-validator-framing.test.ts` — drop `"usezombie.sh/install.sh"` from the `FORBIDDEN_STRINGS` array.
2. `marketing-spec.test.ts` — drop the dedicated negative `it("zero hits on the dead usezombie.sh/install.sh path")` block; keep the existing positive `it` asserting `npm install -g @usezombie/zombiectl` appears in the marketing `src/` (that *is* the website's documented install — the marketing site installs via npm through `<InstallBlock>`, not the curl one-liner).

**Reality note:** these tests run via Vite `import.meta.glob` over the **website package `src/` only** — they cannot reach `~/Projects/docs`. So the docs-site grep (zero `skills.md`, canonical one-liner present) is verified by the Eval Commands (E4/E5) in the docs repo, not the website test. The original spec's claim that `marketing-spec.test.ts` greps the docs repo was infeasible and is corrected here.

### §6 — `InstallBlock.tsx` fixture verify

The design-system test fixture at `ui/packages/design-system/src/design-system/InstallBlock.test.tsx:7` already references `https://usezombie.sh/install | bash`. The new spec settles on `curl -fsSL https://usezombie.sh | bash` (no `/install` path — bare root). The fixture either updates to the new canonical form or stays — implementer's call, but the spec's preference is to match the docs verbatim so the design-system gallery doesn't ship a different one-liner than the docs.

---

## Interfaces

```
# Public CLI contract — invariant across script versions (POSIX; PowerShell follow-up)
curl -fsSL https://usezombie.sh | bash
curl -fsSL https://usezombie.sh | bash -s v0.X.Y          # version-pinned
curl -fsSL https://usezombie.sh | bash -s -- --force       # reinstall, no prompt
USEZOMBIE_INSTALL=/opt/uz curl ... | bash                  # custom install dir
USEZOMBIE_HOST=claude curl ... | bash                      # force agent host

# Exit codes (install.sh)
0  - install succeeded; zombiectl on PATH; skill installed
1  - network unreachable / DNS failure (npm registry unreachable)
2  - zombiectl npm install failed (non-network registry / install error)
3  - install prefix not writable (USEZOMBIE_INSTALL)
4  - host detection ambiguous (multiple host binaries found, no override given)
5  - Node toolchain missing (node/npm not on PATH) — install Node.js >=18
```

No new HTTP/REST surface on the usezombie service. No OpenAPI changes. No new CLI subcommands on `zombiectl` itself (the installers are pre-zombiectl bootstrap).

---

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| DNS NXDOMAIN on `usezombie.sh` | Domain not provisioned / DNS lag during cutover | Pre-launch: docs continue to show the npm + git-clone paths as parallel options, not standalone fallbacks. Launch: `make verify-dns` ping (§Eval Commands) blocks the PR from merging until DNS is global. |
| TLS cert validation failure | Cert misissued / not auto-renewing | Cloudflare Pages auto-issues + auto-renews; if a custom provider, surface in `ui/usezombie.sh/README.md` with the rotation SLA. Script aborts with exit 1 + actionable error. |
| `node`/`npm` toolchain missing | Host has no Node runtime | Exit 5 with an actionable "install Node.js ≥18 from nodejs.org" message. zombiectl is a Node CLI — no tarball fallback can run it, so the script does not pretend one exists. Don't silently succeed. |
| `npm install` fails (non-network) | Registry 4xx, permission, corrupt cache | Exit 2 with the captured npm error tail. Network/DNS errors in npm's output (`ENOTFOUND`/`ECONNREFUSED`/`getaddrinfo`) are reclassified to exit 1. |
| Host detection ambiguous (multiple hosts on `$PATH`) | User has Claude Code + Amp + Codex installed | Exit 4 with prompt to set `USEZOMBIE_HOST=<one>` and re-run. Don't pick arbitrarily — the install destination differs per host. |
| Partial download (connection drops mid-curl) | Network blip during the curl-pipe-bash | `main() { ... }` wrapper means bash refuses to execute a truncated script; user sees a no-op exit, not a half-installed system. |
| User runs as root needlessly | Habit from other installers | Script detects `$EUID == 0` and warns (not aborts) — root is fine for `/opt/uz`, but not required for `~/.usezombie`. Print the warning, continue. |
| Re-install over existing install | Idempotent re-run | Each run is idempotent: detects an existing `zombiectl` (`${USEZOMBIE_INSTALL}/bin/zombiectl` or on PATH) and prints an "existing install detected — upgrading" line before re-running `npm install -g` (which upgrades to the target version). Interactive shells (`/dev/tty`) prompt upgrade/skip; piped `curl … \| bash` (no TTY) defaults to upgrade. `-s -- --force` reinstalls without prompting. |

---

## Invariants

1. **`usezombie.sh` resolves over IPv4 AND IPv6.** Enforced by §Eval Commands `make verify-dns` running `dig +short A usezombie.sh` + `dig +short AAAA usezombie.sh` and asserting both return non-empty.
2. **`install.sh` exits non-zero on every documented failure mode.** Enforced by `install_test.sh` — one test per failure row (node-missing, npm-failed, network, ambiguous-host, install-dir, partial-download).
3. **The four `~/Projects/docs/*` MDX files contain zero references to `usezombie.sh/skills.md`** after §4. Enforced by the Eval Commands grep (E4) in the docs repo — the website test harness can't reach `~/Projects/docs`, so this is a docs-repo / VERIFY check, not a website unit test.
4. **`usezombie.sh/install.sh` is no longer on the banned-strings list** in either marketing test. Enforced by the §5 edits themselves (the ban is deleted, not shimmed).
5. **`install.sh` (and `install_test.sh`) pass `shellcheck -s bash` with zero warnings.** Verified at VERIFY; the `install_test.sh` `shellcheck_clean` test re-checks `install.sh` in-suite.

---

## Test Specification

| Test | Asserts |
|------|---------|
| `test_install_sh_happy_path_posix` | Fake `npm` + `npx` on `$PATH`, run `install.sh`; argv passed to `npm install -g @usezombie/zombiectl` is exact; argv to `npx skills add` includes `--host=<detected>`; exit code 0; next-command hint printed. |
| `test_install_sh_node_missing` | `node`/`npm` absent on `$PATH`; assert exit code 5 + the "install Node.js ≥18 from nodejs.org" message. |
| `test_install_sh_host_detection_claude` | Fake `claude` on `$PATH`; assert detection picks `claude` and `npx skills add --host=claude` is invoked. |
| `test_install_sh_host_detection_ambiguous` | Fake `claude` + `amp` both on `$PATH`, no `USEZOMBIE_HOST` env; assert exit code 4 + diagnostic prompt. |
| `test_install_sh_host_override` | Fake `claude` + `amp` on `$PATH` with `USEZOMBIE_HOST=codex`; assert override wins (`--host=codex`), exit 0. |
| `test_install_sh_network_failure` | Fake `npm` emitting `ENOTFOUND`/`getaddrinfo` on stderr and exiting non-zero; assert exit code 1 + actionable error. |
| `test_install_sh_npm_install_failed` | Fake `npm` exiting non-zero with a non-network error; assert exit code 2 + npm error tail printed. |
| `test_install_sh_install_dir_not_writable` | `USEZOMBIE_INSTALL=/proc/forbidden`; assert exit code 3. |
| `test_install_sh_version_pin` | `bash -s v0.42.0`; assert the `npm install -g` argv targets `@usezombie/zombiectl@0.42.0`. |
| `test_install_sh_partial_download_safety` | Pipe a truncated `install.sh` to bash; assert bash refuses to execute (no `main` call). |
| `test_install_sh_reinstall_idempotent` | Pre-place a fake `zombiectl` on `$PATH`, run twice; assert each run prints "existing install detected" and exits 0 (upgrade path). |
| `test_install_sh_invalid_version_rejected` | `bash -s vNOPE`; assert exit 2 + "invalid version", npm not called. |
| `test_install_sh_unknown_flag_rejected` | `bash -s -- --bogus`; assert exit 2 + "unknown flag". |
| `test_install_sh_npm_missing_node_present` | `node` present, `npm` absent; assert exit 5 (covers the npm half of the Node check). |
| `test_install_sh_generic_host_no_flag` | No host binary on `$PATH`; assert host=`generic`, `npx skills add` invoked **without** `--host`, generic hint printed, exit 0. |
| `test_install_sh_skill_add_failure_nonfatal` | Fake `npx` exits non-zero; assert install still exits 0 with a "skill install did not complete" warning + manual command (skill failure is non-fatal). |
| `test_install_sh_shellcheck_clean` | `shellcheck -s bash install.sh` exit 0, zero warnings. |
| `test_dns_resolves_a_and_aaaa` *(deferred — live cutover)* | `dig +short A usezombie.sh` and `dig +short AAAA usezombie.sh` both return non-empty. |
| `test_tls_cert_valid` *(deferred — live cutover)* | `curl -fsSL https://usezombie.sh -o /dev/null` exit 0, no `--insecure` needed. |
| `test_marketing_spec_no_skills_md_references` | `grep -rn 'usezombie\.sh/skills\.md' ~/Projects/docs/` returns 0 hits. |
| `test_marketing_spec_canonical_one_liner_documented` | `grep -rn 'curl -fsSL https://usezombie.sh \\| bash' ~/Projects/docs/` returns ≥ 1 hit. |
| `test_install_block_fixture_matches_canonical` | The `InstallBlock.test.tsx` fixture command matches the docs' canonical one-liner verbatim. |

---

## Acceptance Criteria

- [x] `usezombie.sh` resolves over A + AAAA — verified May 22, 2026 (`dig +short A usezombie.sh` → `64.29.17.1`, `216.198.79.1`). Live cutover landed via M75_002 (Vercel serve), not the originally-planned Cloudflare Pages.
- [x] TLS valid — verified May 22, 2026: `curl -fsSL https://usezombie.sh -o /dev/null -w "%{http_code}\n"` → `200`, no `--insecure`.
- [x] `install.sh` happy-path in a clean container — verified May 22, 2026 in `node:20-bookworm`: `curl -fsSL https://usezombie.sh | bash` → exit 0, `zombiectl --version` → exit 0. Two findings (neither an `install.sh` defect — the script behaved as designed): (a) `npx skills add usezombie/skills` cloned the repo but reported **"No skills found"** — the public `usezombie/skills` repo has no valid `SKILL.md` yet (pending M69_001), so the skill half of the one-liner installs nothing today; `install.sh` correctly treated this as non-fatal and printed the manual fallback. (b) npm publishes **`@usezombie/zombiectl@0.3.1`** while the repo is at **`0.37.0`** — the stale-publish gap flagged out-of-scope in Scope-amendment 2 persists; a documented-one-liner user gets the ancient CLI.
- [x] Shellcheck clean — `shellcheck -s bash ui/usezombie.sh/dist/install.sh ui/usezombie.sh/install_test.sh` → clean
- [x] `install_test.sh` passes — `bash ui/usezombie.sh/install_test.sh` → 43 passed, 0 failed
- [x] *(docs-repo PR)* No `usezombie.sh/skills.md` references remain in live docs — verified May 22, 2026: the sole grep hit is `changelog.mdx:147`, a historical `<Update>` entry (archived-not-rewritten per CHANGELOG voice), not a live curl path.
- [x] *(docs-repo PR)* Canonical one-liner documented — verified May 22, 2026: `grep -rn 'curl -fsSL https://usezombie.sh | bash' ~/Projects/docs/ | wc -l` → 3 (≥ 1).
- [x] marketing-spec tests pass — `make test-unit-website` → 148 passed; `InstallBlock.test.tsx` → 10 passed
- [x] `make harness-verify` — ALL GATES GREEN
- [x] `gitleaks detect` — no leaks found
- [x] No new file over 350 lines — `install.sh` 244, `install_test.sh` 250
- [x] No `as any` / `!` / `@ts-expect-error` introduced — `make lint-website` + `make lint-apps-ds-ctl` pass

---

## Eval Commands (Post-Implementation Verification)

```bash
# E1: DNS + TLS sanity (DEFERRED — live cutover; run from any clean shell)
dig +short A usezombie.sh
dig +short AAAA usezombie.sh
curl -fsSL https://usezombie.sh -o /tmp/install.sh && echo PASS || echo FAIL

# E2: Shellcheck
shellcheck -s bash ui/usezombie.sh/dist/install.sh 2>&1 | tee /tmp/shellcheck.log
test -s /tmp/shellcheck.log && echo FAIL || echo PASS

# E3: Local script test
bash ui/usezombie.sh/install_test.sh && echo PASS || echo FAIL

# E4: Docs grep — zero usezombie.sh/skills.md references
test "$(grep -rn 'usezombie\.sh/skills\.md' ~/Projects/docs/ 2>/dev/null | wc -l)" = "0" && echo PASS || echo FAIL

# E5: Docs grep — canonical one-liner present
test "$(grep -rn 'curl -fsSL https://usezombie.sh | bash' ~/Projects/docs/ 2>/dev/null | wc -l)" -ge "1" && echo PASS || echo FAIL

# E6: Marketing-spec test
(cd ui/packages/website && bun run test src/marketing-spec.test.ts) && echo PASS || echo FAIL

# E7: Containerized end-to-end (POSIX) — DEFERRED until live cutover
docker run --rm ubuntu:24.04 bash -c "apt-get update && apt-get install -y curl nodejs npm && curl -fsSL https://usezombie.sh | bash && zombiectl --version" && echo PASS || echo FAIL

# E9: Gitleaks
gitleaks detect 2>&1 | tail -3

# E10: 350L gate (exempts .md)
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 { print "OVER: " $2 ": " $1 " lines (limit 350)" }'
```

---

## Dead Code Sweep

**1. Orphaned references — zero remaining `usezombie.sh/skills.md` mentions in user-facing surfaces.**

| Source surface | Grep | Expected |
|---|---|---|
| Docs site | `grep -rn 'usezombie\.sh/skills\.md' ~/Projects/docs/` | 0 matches |
| Main repo MDX/MD | `grep -rn 'usezombie\.sh/skills\.md' docs/ ui/` | 0 matches (architecture docs may keep one mention if it's flagged as the OLD plan, but the cleaner move is to remove) |
| Banned-strings array | `grep -n 'usezombie\.sh/install\.sh' ui/packages/website/src/marketing-spec.test.ts` | 0 matches (entry deliberately removed) |

**2. Files deleted by this spec.** None — every file in §Files Changed is NEW or EDIT. The cleanup is reference-level, not file-level.

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does |
|------|-------|--------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits `install_test.sh` coverage against the in-scope rows in this spec's Test Specification. Catches missing negative paths (e.g., partial-download safety, ambiguous-host case). |
| After tests pass, still before CHORE(close) | `/review` | Adversarial diff review against this spec + `docs/architecture/user_flow.md` §8 + the resend-cli reference. Particular targets: TOCTOU between version check and download, race between two concurrent installs on the same machine, install-dir write atomicity, shell-injection surface in `USEZOMBIE_INSTALL` env var. |
| After `gh pr create` | `/review-pr` | Post-merge-diff review on the open PR. The DNS + TLS lanes will require live verification — record the verification run in PR Session Notes. |

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Shellcheck | `shellcheck -s bash ui/usezombie.sh/dist/install.sh ui/usezombie.sh/install_test.sh` | clean | ✅ |
| install.sh tests | `bash ui/usezombie.sh/install_test.sh` | 43 passed, 0 failed | ✅ |
| Docs sweep (negative) *(docs-repo PR)* | `grep -rn 'usezombie\.sh/skills\.md' ~/Projects/docs/ \| wc -l` | {docs PR} | |
| Docs sweep (positive) *(docs-repo PR)* | `grep -rn 'curl -fsSL https://usezombie.sh \| bash' ~/Projects/docs/ \| wc -l` | {docs PR} | |
| Marketing + InstallBlock | `make test-unit-website`; `InstallBlock.test.tsx` | 148 passed; 10 passed | ✅ |
| Lint | `make lint-website` + `make lint-apps-ds-ctl` | passed | ✅ |
| Harness | `make harness-verify` | ALL GATES GREEN | ✅ |
| Gitleaks | `gitleaks detect` | no leaks found | ✅ |
| DNS A/AAAA | `dig +short A usezombie.sh; dig +short AAAA usezombie.sh` | `64.29.17.1`, `216.198.79.1` (May 22, 2026) | ✅ |
| TLS | `curl -fsSL https://usezombie.sh -w "%{http_code}\n"` | `200` (May 22, 2026) | ✅ |
| Containerized E2E POSIX | E7 in Eval Commands | `node:20-bookworm`: one-liner exit 0, `zombiectl --version` exit 0 (May 22, 2026) — but skill-add found no skills (M69_001 pending) and npm serves 0.3.1 vs repo 0.37.0 | ✅ (script) / ⚠️ (skills+publish, see Acceptance) |

---

## Discovery (consult log)

**May 18, 2026 — spec authored after PR #330 / M71_001 P2 cycle.** Captain flagged `usezombie.sh/skills.md` as broken (DNS NXDOMAIN). Probe confirmed: `curl: (6) Could not resolve host`. Reference projects surveyed: resend-cli (closest match — two scripts at two URLs, bash + PowerShell), bun (`bun.sh/install` + `bun.sh/install.ps1`), deno (`deno.land/install.sh` + `deno.land/install.ps1`), starship (`starship.rs/install.sh` + `starship.rs/install.ps1`), rustup (`sh.rustup.rs` + `win.rustup.rs`). Every adjacent project uses the two-URL pattern, not UA-sniffed content negotiation. M75 follows the convention.

The M51/M72 plan (a single domain serving per-skill `*.md` files for `curl ... > SKILL.md`) is superseded — Captain's intent is "one URL, install both `zombiectl` and `npx skills add`". The skill-per-MD distribution path can return as a sibling subpath in a future spec if needed; not in scope here.

**May 22, 2026 — CHORE(close), retroactive.** The implementation PR (#337) merged May 21, 2026 but this spec was never moved out of `active/` and its Status was left `IN_PROGRESS`. Closing it out now: Status → DONE, spec moved `active/` → `done/`. The Scope-amendment 2 deferrals (live DNS/TLS) have since landed via M75_002 (DONE, served on Vercel) and were verified live this date — DNS A/AAAA resolve, TLS valid, `https://usezombie.sh` → `200`. Docs sweep confirmed clean (the sole `skills.md` grep hit is a historical changelog entry). The containerized `curl … | bash` happy-path smoke was then run in `node:20-bookworm` — one-liner exit 0, `zombiectl --version` exit 0 — so every acceptance check is now satisfied. The smoke also surfaced two product gaps that are out of M75's scope but worth tracking: (1) `npx skills add usezombie/skills` finds no valid `SKILL.md` because the public skills repo content is pending **M69_001**, so the skill half of the one-liner is a no-op today; and (2) npm still serves `@usezombie/zombiectl@0.3.1` against a `0.37.0` repo (the stale-publish gap from Scope-amendment 2 — needs a CLI republish task). Both mean a user running the documented one-liner today gets an ancient CLI and zero skills; neither is an `install.sh` defect.

---

## Out of Scope

- **Windows PowerShell installer** (`install.ps1`, `install_test.ps1`, the `/install.ps1` route, the PowerShell one-liner in docs, the PS-floor exit-6 path) — deferred to a follow-up spec per Scope-amendment 3 (Indy decision, May 21, 2026). WSL users use the POSIX one-liner verbatim until then.
- **Live DNS/TLS provisioning** — was deferred to a post-merge cutover per Scope-amendment 2; it has since landed via M75_002 (DONE) on Vercel rather than the originally-planned Cloudflare Pages. DNS + TLS + the containerized E2E smoke were all verified May 22, 2026 (see Acceptance Criteria) — no acceptance checks remain un-run.
- **Authenticated install paths** (private skills, tenant-scoped distribution) — separate workstream when private skills land.
- **Windows installer for non-PowerShell shells** (cmd.exe, WSL bash) — WSL users use the POSIX one-liner verbatim; cmd.exe users use PowerShell. No third path.
- **Skill-by-name fetching at `usezombie.sh/skills/<name>.md`** — superseded by the one-URL plan. If a future spec wants per-skill curl-able URLs, it adds a sibling subpath; this spec doesn't reserve one.
- **`brew install usezombie/cli/zombiectl`** (Homebrew tap) — separate spec; the one-liner is the primary install path, brew is a parallel option for users who already manage everything via brew.
- **DNS/TLS provider migration** — implementer picks one provider (Cloudflare Pages default per §3); migrating between providers is not in scope.
- **PostHog `install_started` / `install_succeeded` telemetry** — the install scripts are unauthenticated and have no PostHog client; instrumenting them needs a separate signed-request shape. Defer.
