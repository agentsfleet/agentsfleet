<!--
SPEC AUTHORING RULES (load-bearing — do not delete):
- No time/effort/hour/day estimates. Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies sequence.
- Enforced by SPEC TEMPLATE GATE (docs/gates/spec-template.md) + scripts/audit-spec-template.sh.
-->

# M75_002: usezombie.sh serves the installer, not the marketing site

**Prototype:** v2.0.0
**Milestone:** M75
**Workstream:** 002
**Date:** May 22, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — `curl -fsSL https://usezombie.sh | bash` is the install path advertised on the live marketing site (M78 hero); it currently returns HTML, so the installer is broken for every visitor.
**Categories:** INFRA
**Batch:** B1 — sequence after the platform decision (Cloudflare Pages vs Vercel) below.
**Branch:** feat/m75-002-usezombie-sh-serve
**Depends on:** M75_001 (active) — owns the `usezombie.sh` domain + the static `ui/usezombie.sh/dist/` installer files. This fixes how that output is served.
**Provenance:** agent-generated (pre-spec) — Orly diagnosis during the M79 `/design-consultation` session, May 22, 2026 (live Vercel + repo inspection).

**Canonical architecture:** `playbooks/014_usezombie_sh_deploy/001_playbook.md` (the static-installer deploy) + `ui/usezombie.sh/dist/` (the `cloudflare-pages.md`-format `_redirects`/`_headers`).

---

## Implementing agent — read these first

1. `ui/usezombie.sh/dist/` — the deployable: `install.sh` + `_redirects` (`/ → /install.sh 200`) + `_headers` (shellscript content-type). **Cloudflare Pages format** — Vercel ignores both files.
2. `playbooks/014_usezombie_sh_deploy/001_playbook.md` — the intended static, no-build deploy; notes the project was repurposed from the old `usezombie-agents-sh` Vite app.
3. Live state (verified May 22, 2026): the Vercel project `usezombie-agents-sh` serves the domain `usezombie.sh`; `framework` was `vite` (now `Other`/null after a session fix), `rootDirectory=ui/usezombie.sh/dist/`. The current production deploy was built under an **old config** and serves the marketing-site SPA (`index.html`) for every path.

---

## PR Intent & comprehension handshake

- **PR title (eventual):** Serve the usezombie.sh installer (static), not the marketing-site SPA
- **Intent (one sentence):** `curl -fsSL https://usezombie.sh` returns the install shell script (not an HTML page), via the chosen platform, so the advertised one-line install works.
- **Handshake (agent fills at PLAN):** restate intent + `ASSUMPTIONS I'M MAKING: …`. Load-bearing assumption: **the platform decision in Discovery is settled before EXECUTE** (Cloudflare Pages where the `_redirects`/`_headers` already work, or Vercel with a `vercel.json`). A mismatch → STOP.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — universal; RULE NDC (remove the dead-platform config once the platform is chosen), RULE NLR.
- Standard set only otherwise — this is static-hosting config, no Zig/SQL/API surface.

---

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| MILESTONE-ID | yes | config/JSON files outside `docs/` carry no `M75`/`§` tokens. |
| UFS | minor | the `/ → /install.sh` rule + shellscript content-type live in one config file; no cross-runtime literal duplication. |
| File & Function Length | no | static config only; no source over the cap. |
| ZIG / SCHEMA / ERROR REGISTRY / UI / DESIGN TOKEN / LOGGING | no | no such surface touched. |

---

## Overview

**Goal (testable):** `curl -fsSL https://usezombie.sh` returns a body starting `#!/usr/bin/env bash` with content-type `text/x-shellscript`, and `curl https://usezombie.sh/install.sh` returns the same — served from `ui/usezombie.sh/dist/`, not the website SPA.

**Problem (user-facing):** `https://usezombie.sh` returns the marketing-site HTML (`<title>usezombie</title>`) for `/`, `/install.sh`, and arbitrary paths — a SPA catch-all, not the installer. `curl … | bash` pipes HTML into bash and fails. The marketing hero advertises this command, so first-touch installs are broken.

**Solution summary:** Serve the static `dist/` installer on one platform with the root rewrite (`/ → /install.sh`) and shellscript content-type honored. Two viable platforms (Discovery decides): Cloudflare Pages (where the existing `_redirects`/`_headers` already apply) or Vercel (add a `vercel.json` with the rewrite + headers, and ship a fresh production build so the project stops serving the old SPA output).

---

## Prior-Art / Reference Implementations

- **Static install host** → `playbooks/014_usezombie_sh_deploy/001_playbook.md` + the `ui/usezombie.sh/dist/` Cloudflare-Pages output. **Alignment:** keep the same `dist/` artifact. **Divergence:** if Vercel is chosen, translate the Cloudflare `_redirects`/`_headers` into a `vercel.json` (Vercel does not read those files).

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `ui/usezombie.sh/dist/vercel.json` | CREATE (Vercel path only) | rewrite `/ → /install.sh` + shellscript content-type + short cache, replacing the Cloudflare-only `_redirects`/`_headers` Vercel ignores. |
| Vercel project `usezombie-agents-sh` settings + a fresh production deploy | CONFIG (Vercel path) | framework `Other` (done), serve `dist/`; redeploy so production stops serving the stale SPA build. Not a repo file. |
| Cloudflare Pages project + DNS for `usezombie.sh` | CONFIG (Cloudflare path) | if chosen instead: deploy `dist/` on Pages (honors `_redirects`/`_headers`); detach the domain from the Vercel project. Not a repo file. |
| `ui/usezombie.sh/dist/_redirects`, `_headers` | DELETE | RULE NDC — Vercel ignores these Cloudflare-Pages files; drop them now that Vercel is chosen. |
| `playbooks/014_usezombie_sh_deploy/001_playbook.md` | EDIT | Architecture gate (doc-wins-until-reconciled): reconcile the stale Cloudflare-Pages deploy narrative to the actual Vercel git-integration deploy. |
| `ui/usezombie.sh/README.md` | EDIT | Same reconciliation — the "Deploying" + "Layout" sections describe Cloudflare Pages; correct to Vercel + `vercel.json`. |

---

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** pick one hosting platform and make `dist/` serve correctly there; this is a config fix, not new code.
- **Alternatives considered:** (a) keep both Cloudflare-format files AND a Vercel project — rejected: the current split is exactly why it's broken (Cloudflare rules on a Vercel host). (b) rename `install.sh` to `index.html` to dodge the rewrite — rejected: breaks `/install.sh` and the content-type, and hides intent.
- **Patch-vs-refactor verdict:** **patch** — one platform decision + one config file (or one Pages project), no application code.

---

## Sections (implementation slices)

### §1 — Choose the platform (blocks the rest)
Settle Cloudflare Pages vs Vercel for `usezombie.sh` (Discovery decision). Everything downstream depends on this; do not EXECUTE until it is recorded.

- **Dimension 1.1** — platform recorded in Discovery with rationale → verified by the Discovery entry (no code). **DONE** — Vercel, recorded in Discovery (Indy decision, May 22, 2026).

### §2 — Serve the static installer on the chosen platform
Vercel: add `dist/vercel.json` (rewrite + content-type), framework Other, fresh production build. Cloudflare: deploy `dist/` on Pages, detach the domain from Vercel.

- **Dimension 2.1** — `/install.sh` returns the shell script with `text/x-shellscript` → Test `installer_serves_shellscript_at_install_path`
- **Dimension 2.2** — `/` (bare root) returns the same shell script body (rewrite applied) → Test `installer_serves_shellscript_at_root`

### §3 — Remove the dead-platform config (RULE NDC)
Once the platform is chosen, drop the other platform's now-dead config (the Cloudflare `_redirects`/`_headers` if Vercel-only, or the Vercel project/`vercel.json` if Cloudflare).

- **Dimension 3.1** — repo + hosting carry exactly one platform's serving config → verified by grep + the live smoke check.

---

## Interfaces

```
GET https://usezombie.sh/             -> 200, body `#!/usr/bin/env bash …`, content-type text/x-shellscript
GET https://usezombie.sh/install.sh   -> 200, same body + content-type
```

No application API surface; this is static-hosting behaviour only.

---

## Failure Modes

| Mode | Cause | Handling (observable) |
|------|-------|------------------------|
| Root returns HTML | rewrite `/ → /install.sh` not applied (Cloudflare file on Vercel) | the chosen platform's rewrite config makes `/` return the script; smoke check fails until then. |
| Stale SPA served | production deploy built under old config / edge cache | fresh production build + cache purge; verify with an uncached probe path. |
| Wrong content-type | `_headers` ignored by host | platform config sets `text/x-shellscript`; browsers don't force a download, `curl | bash` is unaffected either way. |
| Both platforms serve it | domain attached to two hosts | detach from the not-chosen platform (RULE NDC / §3). |

---

## Invariants

1. `usezombie.sh` serves the install script (not the SPA) at both `/` and `/install.sh` — enforced by a CI/post-deploy smoke check asserting the body starts `#!/usr/bin/env bash`.
2. Exactly one hosting platform owns the domain — enforced by §3 cleanup + the alias check.

---

## Test Specification

| Test | Asserts |
|------|---------|
| `installer_serves_shellscript_at_install_path` | `curl -fsSL https://usezombie.sh/install.sh` body starts `#!/usr/bin/env bash`; content-type `text/x-shellscript`. |
| `installer_serves_shellscript_at_root` | `curl -fsSL https://usezombie.sh/` returns the same script body (root rewrite applied), not HTML. |

Regression: `ui/usezombie.sh/install_test.sh` still passes against the served script. No application tests touched.

---

## Acceptance Criteria

- [ ] `curl -fsSL https://usezombie.sh` returns `#!/usr/bin/env bash` (not `<!doctype html>`) — verify: `curl -fsSL https://usezombie.sh | head -1`
- [ ] `curl -fsSL https://usezombie.sh/install.sh` returns the script with `text/x-shellscript` — verify: `curl -sSI https://usezombie.sh/install.sh | grep -i content-type`
- [ ] Exactly one platform serves the domain (no Cloudflare+Vercel split) — verify: alias/DNS check
- [ ] Dead-platform config removed (RULE NDC) — verify: `git grep` for the dropped files
- [ ] `gitleaks detect` clean

---

## Eval Commands (post-implementation)

```bash
# E1: root serves the script (the bash-pipe contract)
curl -fsSL https://usezombie.sh | head -1   # expect: #!/usr/bin/env bash
# E2: /install.sh content-type
curl -sSI https://usezombie.sh/install.sh | grep -i content-type   # expect: text/x-shellscript
# E3: uncached probe is NOT the SPA
curl -sS "https://usezombie.sh/probe-$(date +%s)" | head -1   # expect: 404 / script, NOT <!doctype html>
# E4: gitleaks
gitleaks detect 2>&1 | tail -3
```

---

## Dead Code Sweep

| File to remove | Verify | When |
|----------------|--------|------|
| The not-chosen platform's config (`_redirects`/`_headers` if Vercel-only; `vercel.json` if Cloudflare) | `git grep -l "_redirects\|vercel.json" ui/usezombie.sh` shows only the chosen platform's files | §3, after the platform decision |

---

## Discovery (consult log)

> **RESOLVED (§1): Vercel.** Indy chose Vercel (May 22, 2026). Rationale: the rest of the static front-end stack already ships on Vercel via a `vercel.json` rewrite (`ui/packages/website/vercel.json`), the `usezombie.sh` domain is already verified on the Vercel `usezombie-agents-sh` project (git-linked to `usezombie/usezombie`, production branch `main`, framework `None`, rootDir `ui/usezombie.sh/dist/`), and there is **no Cloudflare Pages project anywhere in the repo** (no `wrangler.*`; the only `cloudflare` hits are `cloudflared` *tunnels* on Fly for the backend, unrelated). The Cloudflare Pages framing in `playbooks/014_usezombie_sh_deploy/001_playbook.md` + `ui/usezombie.sh/README.md` was authored but never executed — it is **stale doc** and is reconciled to Vercel as part of this work (Architecture gate: doc-wins-until-reconciled). Cloudflare was rejected: it would stand up a second hosting platform and move DNS off Vercel for a single static file.
>
> **OPEN DECISION (now closed): Cloudflare Pages vs Vercel for usezombie.sh.** The `dist/` files were authored for **Cloudflare Pages** (`_redirects`/`_headers`, comments cite `cloudflare-pages.md`); the domain landed on a **Vercel** project (`usezombie-agents-sh`) instead. Resolved to Vercel above.

**Diagnosis (Orly, May 22, 2026 — live + repo inspection):**
- `usezombie.sh` is aliased to the Vercel `usezombie-agents-sh` production deploy (`main@50484f8`); `server: Vercel`.
- Every path (`/`, `/install.sh`, fresh `/probe-…`) returns the marketing SPA `index.html` (`<title>usezombie</title>`, 200) — the production deploy was built under an **old config** and serves the website, not `dist/`.
- The `vite build` error that failed all deploys is **fixed** (project `framework` set to Other/null this session); production builds are READY again. The serving issue is separate.
- Vercel ignores Cloudflare `_redirects`/`_headers`, so even once it serves `dist/`, `/ → /install.sh` and the shellscript content-type need a `vercel.json` (Vercel path) — or the files work as-is on Cloudflare Pages (Cloudflare path).

---

## Skill-Driven Review Chain (mandatory)

| When | Skill | Required output |
|------|-------|-----------------|
| Before CHORE(close) | `/review` | Clean OR findings dispositioned. |
| After `gh pr create` | `/review-pr` | Comments addressed. |
| After every push | `kishore-babysit-prs` | Greptile walked + triaged. |

(`/write-unit-test` is N/A — no application code; the smoke checks are the coverage.)

---

## Verification Evidence

> Filled during VERIFY.

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Root serves script | `curl -fsSL https://usezombie.sh \| head -1` | {paste} | |
| install.sh content-type | `curl -sSI https://usezombie.sh/install.sh` | {paste} | |
| Uncached probe not SPA | Eval E3 | {paste} | |
| Single platform | alias/DNS check | {paste} | |
| Gitleaks | `gitleaks detect` | {paste} | |

---

## Out of Scope

- The marketing website itself (M79 / `ui/packages/website`) — unrelated; that surface is healthy.
- Any rewrite of the `install.sh` script contents — this spec only fixes how it is served.
- The `usezombie-agents-sh` project's legacy name — cosmetic; rename is optional follow-up.
