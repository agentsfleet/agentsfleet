# Playbook — `usezombie.sh` installer domain

**Updated:** May 21, 2026
**Owner:** Human (one-time Cloudflare Pages project + custom domain)
**Prerequisite:** Cloudflare account access to the `usezombie.sh` zone (already delegated — nameservers `clint.ns.cloudflare.com` / `natasha.ns.cloudflare.com`); the Cloudflare GitHub App connected to `usezombie/usezombie` (already used by `usezombie-website` / `usezombie-app` / `usezombie-agents-sh`).

## Why this playbook exists

`https://usezombie.sh` serves the one-URL installer (`curl -fsSL https://usezombie.sh | bash`). The deployable is a static directory (`ui/usezombie.sh/dist/` — `install.sh` + `_redirects` + `_headers`); there is no build step.

It deploys the **same way as the other Pages sites**: a **git-connected** Cloudflare Pages project. Cloudflare's GitHub App builds + deploys a **preview** on every PR (and comments the URL) and **production** on merge to `main`. No GitHub Actions workflow, no `wrangler`, no Cloudflare credentials in CI — Cloudflare's own GitHub auth handles it. The merge is gated by the `lint-usezombie-sh` job (shellcheck + `install_test.sh`), so a broken installer never reaches `main`.

## Sequence

```
1. (once)  create the git-connected `usezombie-sh` Pages project
2. (once)  attach usezombie.sh as a custom domain  -> Cloudflare writes apex A/AAAA + TLS
3. (per change)  open a PR -> Cloudflare auto-deploys a preview; merge to main -> auto prod
4. (verify)  dig + curl the live domain
```

## Human vs Agent split

| Step | Owner | Why |
|------|-------|-----|
| Create git-connected Pages project | Human | One-time Cloudflare dashboard action |
| Attach `usezombie.sh` custom domain | Human | Cloudflare auto-provisions apex A/AAAA + TLS on the existing zone |
| Deploy | Cloudflare | Automatic — preview on PR, production on merge to `main` |
| Verify live DNS/TLS | Agent | Read-only `dig` + `curl` |

---

## Step 1 — Create the git-connected Pages project

Cloudflare dashboard → **Workers & Pages → Create → Pages → Connect to Git** → `usezombie/usezombie`.

| Setting | Value |
|---------|-------|
| Project name | `usezombie-sh` |
| Production branch | `main` |
| Framework preset | None |
| Build command | *(empty — no build)* |
| Build output directory | `ui/usezombie.sh/dist` |
| Path-based build watching (optional) | `ui/usezombie.sh/**` so unrelated pushes don't trigger a (no-op) redeploy |

(There is already a `usezombie-agents-sh` project that fronted the old "forward to usezombie.com/agents" approach. `usezombie.sh` now serves the installer instead — either repurpose that project's domain binding or create `usezombie-sh` fresh and move the custom domain. Either is fine; the served output dir is what matters.)

## Step 2 — Attach the custom domain

In the project → **Custom domains → Set up a domain → `usezombie.sh`**. Cloudflare writes the apex `A`/`AAAA` records and issues the TLS certificate automatically (the zone is already on this account). No manual DNS edits.

## Step 3 — Deploy

Automatic. Open a PR touching `ui/usezombie.sh/**` → Cloudflare posts a preview URL on the PR. Merge to `main` → Cloudflare deploys production → `https://usezombie.sh`. The `_headers` 5-minute `Cache-Control` propagates a bump globally within minutes. To change the served script, edit `ui/usezombie.sh/dist/install.sh` and merge.

## Step 4 — Verify (live cutover acceptance)

```bash
dig +short A usezombie.sh                 # non-empty
dig +short AAAA usezombie.sh              # non-empty
curl -fsSL https://usezombie.sh -o /tmp/install.sh    # HTTP 200, valid TLS, no --insecure
curl -fsSL https://usezombie.sh | head -1             # -> #!/usr/bin/env bash
```

## Prerequisite for a meaningful end-to-end install

The script runs `npm install -g @usezombie/zombiectl`. At authoring time npm lagged at `0.3.0` vs the repo's `0.37.0`, and GitHub Releases topped out at `v0.4.0`. Publish a current `@usezombie/zombiectl` to npm before treating a live `curl … | bash` install as the real end-to-end path — that is a separate release task, not part of this wiring.
