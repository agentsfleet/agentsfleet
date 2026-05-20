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

# M75_001: `usezombie.sh` — one-URL cross-platform installer (POSIX + PowerShell)

**Prototype:** v2.0.0
**Milestone:** M75
**Workstream:** 001
**Date:** May 18, 2026
**Status:** PENDING
**Priority:** P2 — not blocking any other workstream, but the broken curl path is an onboarding hazard (every doc page that names `usezombie.sh/skills.md` today serves a DNS failure to readers). Captain decision May 18, 2026 to make `usezombie.sh` the canonical entrypoint, replacing the M51/M72 plan that had it serving per-skill `*.md` files.
**Categories:** CLI, DOCS, INFRA
**Batch:** B1
**Branch:** feat/m75-001-usezombie-sh-install-domain (to be created)
**Depends on:** None — greenfield install path. M71_001 (DONE) and M72_001 (DONE) shipped the docs that today reference the broken `usezombie.sh/skills.md`; this spec fixes those references as part of §3.
**Provenance:** agent-generated. Captain in-session decision on May 18, 2026 after PR #330 (M71_001 P2) cycle surfaced that `usezombie.sh/skills.md` is referenced in four user-facing docs but the domain doesn't resolve (DNS NXDOMAIN). Captain settled the intent as "`https://usezombie.sh` should start the install of zombiectl AND `npx skills add`" — both, one URL, classic one-liner installer.

**Canonical architecture:** `docs/architecture/user_flow.md` §8 (cold-machine bootstrap) — already names `https://usezombie.sh` as the host-skill install entry; this spec implements that contract on disk and wires it to the live domain.

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

- **PR title (eventual):** usezombie.sh: one-URL cross-platform installer (POSIX + PowerShell)
- **Intent (one sentence):** `https://usezombie.sh` resolves and installs `zombiectl` + the platform-ops skill in one documented command on Linux/macOS/Windows, retiring the broken `usezombie.sh/skills.md` curl path.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate the intent in your own words and list the assumptions you proceed on (`ASSUMPTIONS I'M MAKING: …`). Three to name: (1) the canonical one-liner is the bare-root form `curl -fsSL https://usezombie.sh | bash` (no `/install` path); (2) DNS/TLS lands out-of-tree and must be verified live before merge; (3) the M51 `usezombie.sh/install.sh` banned-strings entry is deliberately removed, not worked around. A mismatch with the Intent above → STOP and reconcile before any edit.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — universal repo discipline. RULE NDC (no dead code), RULE NLR (touch-it-fix-it), RULE UFS (unique-functionality strings — the install one-liner is shared between the script comments, both docs sites, and the `marketing-spec.test.ts` positive assertion; pin once).
- **`docs/greptile-learnings/RULES.md` RULE NLG** — no "legacy" framing for the M51 banned-strings entry being removed. The old `usezombie.sh/skills.md` plan was superseded, not deprecated; remove the entry cleanly without legacy-shim prose.
- **`docs/REST_API_DESIGN_GUIDELINES.md`** — N/A; this spec adds no HTTP handlers on the usezombie service.
- **`docs/SCHEMA_CONVENTIONS.md`** — N/A; no schema changes.
- **`docs/ZIG_RULES.md`** — N/A; `infra/install/install.sh` is bash, `install.ps1` is PowerShell, no Zig touched.
- **`docs/AUTH.md`** — N/A; install path is unauthenticated. Authenticated install variants are explicitly Out of Scope.
- Shellcheck — `install.sh` must pass `shellcheck -s bash install.sh` with zero warnings. PowerShell equivalent: `Invoke-ScriptAnalyzer install.ps1` must surface zero `Error` / `Warning` rules.

---

## Applicable Gates

> Which Action-Triggered Guards this PR trips, and how each stays clean. Blast radius: `infra/install/*.{sh,ps1,md}`, out-of-tree DNS/TLS config, cross-repo docs `.mdx`, and two `.ts` marketing-spec tests (+ one `.tsx` fixture verify). No Zig, no schema.

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | no | `install.sh` is bash, `install.ps1` is PowerShell — no Zig touched. |
| PUB / Struct-Shape | no | no Zig surface. |
| File & Function Length (≤350/≤50/≤70) | yes | `.sh` is in the length surface — use a `main()` wrapper + small helpers; keep each script ≤350 lines and shell functions ≤50; split if the script approaches the cap. |
| UFS (repeated/semantic literals) | yes | the install one-liner is shared across script comments, both docs sites, and the `marketing-spec.test.ts` positive assertion — pin once and reference verbatim (RULE UFS). |
| UI Substitution / DESIGN TOKEN | no | `InstallBlock.test.tsx` is a fixture-string verify, not a component-markup change. |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | LOGGING: yes | `.sh` is in the LOGGING surface; script output is user-facing install progress (not the app's structured logger) — no secrets echoed, errors actionable and to stderr, color gated on `[[ -t 1 ]]`. ERROR REGISTRY/LIFECYCLE/SCHEMA: no (numeric exit codes 0–5, no `UZ-*` codes, no Zig lifecycle, no schema). |

---

## Overview

**Goal (testable):** a user on Linux, macOS, or Windows can run a single documented one-liner that downloads `zombiectl`, installs it on PATH, runs `npx skills add usezombie/usezombie --host=<detected>`, and prints a "next command" hint. The one-liner resolves DNS, the cert is valid, and the script fails fast with an actionable error on any of: missing node toolchain, network unreachable, install-dir not writable, host detection ambiguous.

**Problem:** Four user-facing doc pages today print `curl -fsSL https://usezombie.sh/skills.md > <path>/SKILL.md` as the curl-fallback install. `usezombie.sh` does not resolve — DNS NXDOMAIN. Every reader who follows the docs literally sees `curl: (6) Could not resolve host: usezombie.sh`. The skill-per-MD path was the M51 plan; Captain superseded it on May 18, 2026 with the one-URL one-liner pattern.

**Solution summary:** Author two installer scripts (`install.sh` for POSIX, `install.ps1` for Windows) under `infra/install/` in the main repo. Wire `usezombie.sh` DNS + TLS + static host to serve `install.sh` content at `/` and `/install.sh`, and `install.ps1` content at `/install.ps1`. Sweep the four docs to print the new one-liners (POSIX one-liner primary, PowerShell one-liner secondary). Remove `usezombie.sh/install.sh` from the `marketing-spec.test.ts` banned-strings list and add a positive assertion that the canonical one-liner appears in at least one user-facing doc page. The `InstallBlock.tsx` test fixture already references the new shape — verify it still aligns.

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
| `infra/install/install.sh` | NEW | POSIX installer. `main() { ... }` wrapper for partial-download safety; detects host (`claude`, `amp`, `codex`, `opencode`, generic); installs `zombiectl` via npm (curl tarball fallback for nodeless environments); runs `npx skills add usezombie/usezombie --host=<detected>`; prints success + next-command hint. |
| `infra/install/install.ps1` | NEW | Windows PowerShell installer. Mirror of `install.sh` adapted to PowerShell idioms (`Set-StrictMode -Version Latest`, `$ErrorActionPreference = 'Stop'`, `Invoke-WebRequest` instead of curl). |
| `infra/install/install_test.sh` | NEW | Black-box dry-run smoke test for `install.sh` — fake `npm`/`npx`/`curl` on `$PATH`, assert the script invokes them with the expected argv, asserts exit codes for each failure mode, asserts the next-command hint is printed on success. |
| `infra/install/install_test.ps1` | NEW | PowerShell counterpart — same assertions adapted to Pester (or a simple PowerShell test harness, implementer choice). |
| `infra/install/README.md` | NEW | One-page contributor doc — what these scripts do, how the DNS/static-host wiring lives outside the repo (provider config not in tree), how to test locally, how to bump pinned versions. |
| `infra/dns/usezombie-sh.tf` or equivalent | NEW (out-of-tree allowed) | DNS A/AAAA records, TLS cert, static-host config. Implementer picks Cloudflare Pages, Vercel, or a generic S3+CloudFront — pick the cheapest path that gives sub-100ms TTFB globally. If this lands in the existing infra repo, link the PR in the Discovery section. |
| `~/Projects/docs/changelog.mdx` | EDIT | Replace the May 17, 2026 entry's `usezombie.sh/skills.md` mention (line ~44) with the new one-liner. Don't rewrite history for prior entries — only touch the line that's actively wrong. |
| `~/Projects/docs/quickstart.mdx` | EDIT | Replace `curl -fsSL https://usezombie.sh/skills.md \` block (~line 74) with the new POSIX + PowerShell one-liner pair. Keep `npx skills add usezombie/usezombie` as a parallel option for users who already have a node toolchain. |
| `~/Projects/docs/cli/install.mdx` | EDIT | Same swap, ~line 48. |
| `~/Projects/docs/zombies/install.mdx` | EDIT | Same swap, ~line 43. |
| `docs/architecture/user_flow.md` | EDIT | §8 verification — the doc already names `https://usezombie.sh` as canonical; verify the install-path subsection is consistent with the new two-script reality (POSIX + PowerShell) and amend if it implied a single script. |
| `ui/packages/website/src/marketing-spec.test.ts` | EDIT | Remove `usezombie.sh/install.sh` from the banned-strings list (was banned per M51 when the plan was different). Add a positive assertion: at least one user-facing doc page contains the canonical one-liner. |
| `ui/packages/website/src/marketing-no-pr-validator-framing.test.ts` | EDIT | Mirror the banned-strings update; this file shares the dead-string list. |
| `ui/packages/design-system/src/design-system/InstallBlock.test.tsx` | VERIFY (no edit expected) | Fixture at line 7 already references `https://usezombie.sh/install | bash`. Verify the new install URL the spec settles on matches verbatim, or update the fixture in this PR. |

> **Anti-pattern guard:** no file in `src/` (Zig), `zombiectl/` (TypeScript CLI), or `ui/packages/app/` is touched by this spec. The CLI install logic lives in the bash/PowerShell scripts; the dashboard surface is untouched.

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** six slices — POSIX installer, PowerShell installer, DNS + static-host wiring, docs sweep, marketing-spec banned-string flip, `InstallBlock` fixture verify. Two scripts at two URLs, no content negotiation.
- **Alternatives considered:** (a) a single script served at one URL with User-Agent content negotiation — rejected; fragile, and no adjacent project does it. (b) keep the per-skill `*.md` curl plan from M51/M72 — superseded by Indy's one-URL decision (May 18, 2026).
- **Patch-vs-refactor verdict:** **neither a patch nor a refactor of existing code** — a greenfield install path. The only edits to existing files are the docs sweep and the banned-strings flip (cleanup of now-wrong references), kept tight rather than bundled with unrelated changes.

---

## Sections (implementation slices)

### §1 — POSIX installer (`install.sh`)

Bash script that bootstraps `zombiectl` + the platform-ops skill on Linux + macOS. **Implementation default**: mirror the resend-cli `install.sh` shape — `main() { ... }` wrapper for partial-download protection, `set -euo pipefail`, colored output gated on `[[ -t 1 ]]`, install-dir defaults to `~/.usezombie` with `USEZOMBIE_INSTALL` env override, version pin via `bash -s vX.Y.Z`. Host detection inspects `$PATH` for `claude`, `amp`, `codex`, `opencode` binaries in that order; falls back to `generic` if none found. Install path: prefer `npm install -g @usezombie/zombiectl` when node is on `$PATH`; fall back to downloading the `zombiectl-{platform}-{arch}` tarball from GitHub Releases and unpacking to `${USEZOMBIE_INSTALL}/bin/`. After zombiectl install, run `npx skills add usezombie/usezombie --host=<detected>` if node is available, else print the manual `git clone` fallback. Print a "next command" hint that depends on detected host (e.g., `/usezombie-install-platform-ops` for Claude Code).

### §2 — Windows installer (`install.ps1`)

PowerShell mirror of §1. **Implementation default**: mirror resend-cli's `install.ps1` shape — `Set-StrictMode -Version Latest`, `$ErrorActionPreference = 'Stop'`, `Invoke-WebRequest` for downloads, `$env:USEZOMBIE_INSTALL` for install-dir override (default `$HOME\.usezombie`), version pin via `$env:USEZOMBIE_VERSION`. Host detection inspects `$env:PATH` for the same four host binaries plus `claude.exe` variants. Install path: same npm-first / tarball-fallback pattern, with `.zip` instead of `.tar.gz`. Skill install: same `npx skills add` call, with the PowerShell equivalent fallback (`git clone` + manual symlink) for nodeless Windows hosts.

### §3 — DNS + static host wiring

`usezombie.sh` resolves over IPv4 + IPv6, serves valid TLS, returns `install.sh` content at `/` (so `curl -fsSL usezombie.sh | bash` works without naming the path) and `/install.sh` (for `wget`-style two-step users), and `install.ps1` content at `/install.ps1` (for the PowerShell one-liner). **Implementation default**: Cloudflare Pages serving a static `_redirects` or `_headers`-driven router off a small build that copies `infra/install/install.{sh,ps1}` into the deploy. Set cache TTLs short (5 min) so a script bump is live globally fast. The provider config lives in the existing infra repo or in `infra/dns/usezombie-sh.tf` — implementer's call.

### §4 — Docs sweep (the four broken pages)

Replace every `curl -fsSL https://usezombie.sh/skills.md \` block in `~/Projects/docs/{changelog,quickstart,cli/install,zombies/install}.mdx` with a two-line block:

- Primary (POSIX): `curl -fsSL https://usezombie.sh | bash`
- Secondary (Windows PowerShell): `irm https://usezombie.sh/install.ps1 | iex`

The `npx skills add usezombie/usezombie` line stays as the "I already have a node toolchain" parallel option — it's not removed, it's positioned alongside the curl one-liner. Run the existing `~/Projects/docs/` build (`mintlify dev` or equivalent) and verify no MDX parse errors.

### §5 — `marketing-spec.test.ts` banned-string flip

Two changes: (1) remove `usezombie.sh/install.sh` (and `usezombie.sh/install`) from the banned-strings array — the M51 ban was based on a now-superseded plan. (2) Add a positive test row asserting that at least one user-facing doc page contains the new canonical one-liner. The positive test grep's against `~/Projects/docs/**/*.mdx` (or whatever path the website's test harness can reach — implementer reads the existing test for the pattern). RULE NDC: no comment carries the M51 framing forward; the entry is removed cleanly.

### §6 — `InstallBlock.tsx` fixture verify

The design-system test fixture at `ui/packages/design-system/src/design-system/InstallBlock.test.tsx:7` already references `https://usezombie.sh/install | bash`. The new spec settles on `curl -fsSL https://usezombie.sh | bash` (no `/install` path — bare root). The fixture either updates to the new canonical form or stays — implementer's call, but the spec's preference is to match the docs verbatim so the design-system gallery doesn't ship a different one-liner than the docs.

---

## Interfaces

```
# Public CLI contract — invariant across script versions
curl -fsSL https://usezombie.sh | bash
curl -fsSL https://usezombie.sh | bash -s v0.X.Y          # version-pinned
USEZOMBIE_INSTALL=/opt/uz curl ... | bash                  # custom install dir

irm https://usezombie.sh/install.ps1 | iex
$env:USEZOMBIE_VERSION = 'v0.X.Y'; irm ... | iex           # version-pinned
$env:USEZOMBIE_INSTALL = 'C:\opt\uz'; irm ... | iex        # custom install dir

# Exit codes (both scripts)
0  - install succeeded; zombiectl on PATH; skill installed (if node available)
1  - network unreachable / DNS failure
2  - GitHub release fetch failed (tarball download)
3  - install-dir not writable
4  - host detection ambiguous (multiple host binaries found, no override given)
5  - node toolchain missing AND tarball fallback failed
```

No new HTTP/REST surface on the usezombie service. No OpenAPI changes. No new CLI subcommands on `zombiectl` itself (the installers are pre-zombiectl bootstrap).

---

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| DNS NXDOMAIN on `usezombie.sh` | Domain not provisioned / DNS lag during cutover | Pre-launch: docs continue to show the npm + git-clone paths as parallel options, not standalone fallbacks. Launch: `make verify-dns` ping (§Eval Commands) blocks the PR from merging until DNS is global. |
| TLS cert validation failure | Cert misissued / not auto-renewing | Cloudflare Pages auto-issues + auto-renews; if a custom provider, surface in `infra/install/README.md` with the rotation SLA. Script aborts with exit 1 + actionable error. |
| `npm` missing AND tarball fetch fails | Air-gapped or restricted-network host | Exit 5 with a printed manual install URL pointing at the GitHub Releases page. Don't silently succeed. |
| Host detection ambiguous (multiple hosts on `$PATH`) | User has Claude Code + Amp + Codex installed | Exit 4 with prompt to set `USEZOMBIE_HOST=<one>` and re-run. Don't pick arbitrarily — the install destination differs per host. |
| Partial download (connection drops mid-curl) | Network blip during the curl-pipe-bash | `main() { ... }` wrapper means bash refuses to execute a truncated script; user sees a no-op exit, not a half-installed system. |
| User runs as root needlessly | Habit from other installers | Script detects `$EUID == 0` and warns (not aborts) — root is fine for `/opt/uz`, but not required for `~/.usezombie`. Print the warning, continue. |
| Re-install over existing install | Idempotent re-run | Each run is idempotent: detects existing `${USEZOMBIE_INSTALL}/bin/zombiectl`, prompts to upgrade or skip. `-s --force` flag bypasses the prompt. |
| Windows PowerShell version too old | PowerShell 5.0 / Windows 7 | Detect `$PSVersionTable.PSVersion.Major < 5`; print upgrade link + exit. PowerShell 5.1+ is the floor. |

---

## Invariants

1. **`usezombie.sh` resolves over IPv4 AND IPv6.** Enforced by §Eval Commands `make verify-dns` running `dig +short A usezombie.sh` + `dig +short AAAA usezombie.sh` and asserting both return non-empty.
2. **Both `install.sh` and `install.ps1` exit non-zero on every documented failure mode.** Enforced by `install_test.sh` + `install_test.ps1` — one test per row in the Failure Modes table.
3. **The four docs/* MDX files contain zero references to `usezombie.sh/skills.md`** after §4. Enforced by a grep assertion in `marketing-spec.test.ts` (positive test added in §5).
4. **`usezombie.sh/install.sh` is no longer on the banned-strings list.** Enforced by the §5 test edit itself — if a future agent re-bans the string, the marketing-spec test fails because the positive assertion stops finding the canonical one-liner.
5. **`install.sh` passes `shellcheck -s bash` with zero warnings.** Enforced as a CI step (added in §3 or §1 — implementer's choice).

---

## Test Specification

| Test | Asserts |
|------|---------|
| `test_install_sh_happy_path_posix` | Fake `npm` + `npx` on `$PATH`, run `install.sh`; argv passed to `npm install -g @usezombie/zombiectl` is exact; argv to `npx skills add` includes `--host=<detected>`; exit code 0; next-command hint printed. |
| `test_install_sh_tarball_fallback` | `npm` absent on `$PATH`, fake `curl` returning a known tarball; assert extraction to `${USEZOMBIE_INSTALL}/bin/`; exit code 0. |
| `test_install_sh_host_detection_claude` | Fake `claude` on `$PATH`; assert detection picks `claude` and `npx skills add --host=claude` is invoked. |
| `test_install_sh_host_detection_ambiguous` | Fake `claude` + `amp` both on `$PATH`, no `USEZOMBIE_HOST` env; assert exit code 4 + diagnostic prompt. |
| `test_install_sh_network_failure` | Fake `curl` returning exit 6 (DNS); assert exit code 1 + actionable error. |
| `test_install_sh_install_dir_not_writable` | `USEZOMBIE_INSTALL=/proc/forbidden`; assert exit code 3. |
| `test_install_sh_version_pin` | `bash -s v0.42.0`; assert the tarball URL contains `v0.42.0`. |
| `test_install_sh_partial_download_safety` | Pipe a truncated `install.sh` to bash; assert bash refuses to execute (no `main` call). |
| `test_install_sh_reinstall_idempotent` | Run twice; assert second run detects existing install and prompts (or skips with `--force`). |
| `test_install_sh_shellcheck_clean` | `shellcheck -s bash install.sh` exit 0, zero warnings. |
| `test_install_ps1_happy_path_windows` | Mirror of `_happy_path_posix` on Windows. |
| `test_install_ps1_old_powershell_rejected` | Fake `$PSVersionTable.PSVersion.Major = 4`; assert exit + upgrade link. |
| `test_dns_resolves_a_and_aaaa` | `dig +short A usezombie.sh` and `dig +short AAAA usezombie.sh` both return non-empty. |
| `test_tls_cert_valid` | `curl -fsSL https://usezombie.sh -o /dev/null` exit 0, no `--insecure` needed. |
| `test_marketing_spec_no_skills_md_references` | `grep -rn 'usezombie\.sh/skills\.md' ~/Projects/docs/` returns 0 hits. |
| `test_marketing_spec_canonical_one_liner_documented` | `grep -rn 'curl -fsSL https://usezombie.sh \\| bash' ~/Projects/docs/` returns ≥ 1 hit. |
| `test_install_block_fixture_matches_canonical` | The `InstallBlock.test.tsx` fixture command matches the docs' canonical one-liner verbatim. |

---

## Acceptance Criteria

- [ ] `usezombie.sh` resolves over A + AAAA — verify: `dig +short A usezombie.sh && dig +short AAAA usezombie.sh`
- [ ] TLS valid — verify: `curl -fsSL https://usezombie.sh -o /dev/null -w "%{http_code}\n"` prints `200`
- [ ] `install.sh` happy-path runs — verify: in a clean container, `curl -fsSL https://usezombie.sh | bash && zombiectl --version`
- [ ] `install.ps1` happy-path runs — verify: in a Windows VM / GitHub Actions Windows runner, `irm https://usezombie.sh/install.ps1 | iex; zombiectl --version`
- [ ] Shellcheck clean — verify: `shellcheck -s bash infra/install/install.sh`
- [ ] `install_test.sh` passes — verify: `bash infra/install/install_test.sh` exit 0
- [ ] `install_test.ps1` passes — verify: `pwsh infra/install/install_test.ps1` exit 0
- [ ] No `usezombie.sh/skills.md` references remain — verify: `grep -rn 'usezombie\.sh/skills\.md' ~/Projects/docs/ | wc -l` == 0
- [ ] Canonical one-liner documented — verify: `grep -rn 'curl -fsSL https://usezombie.sh | bash' ~/Projects/docs/ | wc -l` ≥ 1
- [ ] `(cd ui/packages/website && bun run test)` — marketing-spec tests pass (positive + negative assertions)
- [ ] `make harness-verify` 8/8 green
- [ ] `gitleaks detect` clean
- [ ] No new file over 350 lines
- [ ] No `as any` / `!` / `@ts-expect-error` introduced

---

## Eval Commands (Post-Implementation Verification)

```bash
# E1: DNS + TLS sanity (run from any clean shell — no repo deps)
dig +short A usezombie.sh
dig +short AAAA usezombie.sh
curl -fsSL https://usezombie.sh -o /tmp/install.sh && echo PASS || echo FAIL
curl -fsSL https://usezombie.sh/install.ps1 -o /tmp/install.ps1 && echo PASS || echo FAIL

# E2: Shellcheck
shellcheck -s bash infra/install/install.sh 2>&1 | tee /tmp/shellcheck.log
test -s /tmp/shellcheck.log && echo FAIL || echo PASS

# E3: Local script tests
bash infra/install/install_test.sh && echo PASS || echo FAIL
pwsh infra/install/install_test.ps1 && echo PASS || echo FAIL

# E4: Docs grep — zero usezombie.sh/skills.md references
test "$(grep -rn 'usezombie\.sh/skills\.md' ~/Projects/docs/ 2>/dev/null | wc -l)" = "0" && echo PASS || echo FAIL

# E5: Docs grep — canonical one-liner present
test "$(grep -rn 'curl -fsSL https://usezombie.sh | bash' ~/Projects/docs/ 2>/dev/null | wc -l)" -ge "1" && echo PASS || echo FAIL

# E6: Marketing-spec test
(cd ui/packages/website && bun run test src/marketing-spec.test.ts) && echo PASS || echo FAIL

# E7: Containerized end-to-end (POSIX)
docker run --rm ubuntu:24.04 bash -c "apt-get update && apt-get install -y curl nodejs npm && curl -fsSL https://usezombie.sh | bash && zombiectl --version" && echo PASS || echo FAIL

# E8: Containerized end-to-end (Windows) — runs in CI on windows-latest only
# pwsh -c "iwr https://usezombie.sh/install.ps1 | iex; zombiectl --version"

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
| After implementation, before CHORE(close) | `/write-unit-test` | Audits `install_test.sh` + `install_test.ps1` coverage against the 17 rows in this spec's Test Specification. Catches missing negative paths (e.g., partial-download safety, ambiguous-host case). |
| After tests pass, still before CHORE(close) | `/review` | Adversarial diff review against this spec + `docs/architecture/user_flow.md` §8 + the resend-cli reference. Particular targets: TOCTOU between version check and download, race between two concurrent installs on the same machine, install-dir write atomicity, shell-injection surface in `USEZOMBIE_INSTALL` env var. |
| After `gh pr create` | `/review-pr` | Post-merge-diff review on the open PR. The DNS + TLS lanes will require live verification — record the verification run in PR Session Notes. |

---

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| DNS A/AAAA | `dig +short A usezombie.sh; dig +short AAAA usezombie.sh` | {filled at VERIFY} | |
| TLS | `curl -fsSL https://usezombie.sh -w "%{http_code}\n"` | {filled at VERIFY} | |
| Shellcheck | `shellcheck -s bash infra/install/install.sh` | {filled at VERIFY} | |
| install.sh tests | `bash infra/install/install_test.sh` | {filled at VERIFY} | |
| install.ps1 tests | `pwsh infra/install/install_test.ps1` | {filled at VERIFY} | |
| Docs sweep (negative) | `grep -rn 'usezombie\.sh/skills\.md' ~/Projects/docs/ \| wc -l` | {filled at VERIFY} | |
| Docs sweep (positive) | `grep -rn 'curl -fsSL https://usezombie.sh \| bash' ~/Projects/docs/ \| wc -l` | {filled at VERIFY} | |
| Marketing spec | `(cd ui/packages/website && bun run test src/marketing-spec.test.ts)` | {filled at VERIFY} | |
| Harness | `make harness-verify` | {filled at VERIFY} | |
| Gitleaks | `gitleaks detect` | {filled at VERIFY} | |
| Containerized E2E (POSIX) | E7 in Eval Commands | {filled at VERIFY} | |
| Containerized E2E (Windows) | E8 in Eval Commands | {filled at VERIFY} | |

---

## Discovery (consult log)

**May 18, 2026 — spec authored after PR #330 / M71_001 P2 cycle.** Captain flagged `usezombie.sh/skills.md` as broken (DNS NXDOMAIN). Probe confirmed: `curl: (6) Could not resolve host`. Reference projects surveyed: resend-cli (closest match — two scripts at two URLs, bash + PowerShell), bun (`bun.sh/install` + `bun.sh/install.ps1`), deno (`deno.land/install.sh` + `deno.land/install.ps1`), starship (`starship.rs/install.sh` + `starship.rs/install.ps1`), rustup (`sh.rustup.rs` + `win.rustup.rs`). Every adjacent project uses the two-URL pattern, not UA-sniffed content negotiation. M75 follows the convention.

The M51/M72 plan (a single domain serving per-skill `*.md` files for `curl ... > SKILL.md`) is superseded — Captain's intent is "one URL, install both `zombiectl` and `npx skills add`". The skill-per-MD distribution path can return as a sibling subpath in a future spec if needed; not in scope here.

---

## Out of Scope

- **Authenticated install paths** (private skills, tenant-scoped distribution) — separate workstream when private skills land.
- **Windows installer for non-PowerShell shells** (cmd.exe, WSL bash) — WSL users use the POSIX one-liner verbatim; cmd.exe users use PowerShell. No third path.
- **Skill-by-name fetching at `usezombie.sh/skills/<name>.md`** — superseded by the one-URL plan. If a future spec wants per-skill curl-able URLs, it adds a sibling subpath; this spec doesn't reserve one.
- **`brew install usezombie/cli/zombiectl`** (Homebrew tap) — separate spec; the one-liner is the primary install path, brew is a parallel option for users who already manage everything via brew.
- **DNS/TLS provider migration** — implementer picks one provider (Cloudflare Pages default per §3); migrating between providers is not in scope.
- **PostHog `install_started` / `install_succeeded` telemetry** — the install scripts are unauthenticated and have no PostHog client; instrumenting them needs a separate signed-request shape. Defer.
