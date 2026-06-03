# Playbook — `usezombie.sh` installer domain

**Updated:** May 22, 2026
**Owner:** Human (one-time Vercel project + custom domain — already provisioned)
**Prerequisite:** Vercel team access (`indykishs-projects`); the Vercel GitHub integration connected to `usezombie/usezombie` (already used by `usezombie-website` and `usezombie-agents-sh`).

## Why this playbook exists

`https://usezombie.sh` serves the one-URL installer (`curl -fsSL https://usezombie.sh | bash`). The deployable is a static directory (`ui/usezombie.sh/dist/` — `install.sh` + `vercel.json`); there is no build step.

It deploys the **same way as `usezombie-website`**: a **git-connected** Vercel project. Vercel's GitHub integration builds + deploys a **preview** on every PR (and comments the URL) and **production** on merge to `main`. No GitHub Actions workflow, no Vercel credentials in CI — Vercel's own GitHub auth handles it. The merge is gated by the `lint-usezombie-sh` job (shellcheck + `install_test.sh`), so a broken installer never reaches `main`.

`dist/vercel.json` carries the serving config: the `/ → /install.sh` rewrite (so the bare root pipes into bash) and the `text/x-shellscript` content-type + 5-minute cache. Vercel does **not** read Cloudflare-Pages `_redirects`/`_headers`, which is why the config lives in `vercel.json`.

## Sequence

```
1. (once, done)  git-connected Vercel project `usezombie-agents-sh`, rootDir ui/usezombie.sh/dist
2. (once, done)  usezombie.sh attached as a custom domain  -> Vercel provisions apex DNS + TLS
3. (per change)  open a PR -> Vercel auto-deploys a preview; merge to main -> auto prod
4. (verify)      dig + curl the live domain
```

## Human vs Agent split

| Step | Owner | Why |
|------|-------|-----|
| Create git-connected Vercel project | Human | One-time Vercel dashboard action (done) |
| Attach `usezombie.sh` custom domain | Human | Vercel auto-provisions apex DNS + TLS (done) |
| Deploy | Vercel | Automatic — preview on PR, production on merge to `main` |
| Verify live DNS/TLS | Agent | Read-only `dig` + `curl` |

---

## Step 1 — The git-connected Vercel project (already provisioned)

Vercel dashboard → **Add New → Project → Import** `usezombie/usezombie`. Current config:

| Setting | Value |
|---------|-------|
| Project name | `usezombie-agents-sh` |
| Production branch | `main` |
| Framework preset | Other / None |
| Build command | *(empty — no build)* |
| Root directory | `ui/usezombie.sh/dist` |

(The project was repurposed from the old "forward to usezombie.com/agents" approach; `usezombie.sh` now serves the installer. The serving config lives in `dist/vercel.json`.)

## Step 2 — The custom domain (already attached)

Project → **Settings → Domains → `usezombie.sh`**. Vercel issues the TLS certificate and provisions the apex DNS automatically. No manual DNS edits.

## Step 3 — Deploy

Automatic. Open a PR touching `ui/usezombie.sh/**` → Vercel posts a preview URL on the PR. Merge to `main` → Vercel deploys production → `https://usezombie.sh`. The `vercel.json` 5-minute `Cache-Control` propagates a bump globally within minutes. To change the served script, edit `ui/usezombie.sh/dist/install.sh` and merge.

## Step 4 — Verify (live cutover acceptance)

```bash
dig +short A usezombie.sh                 # non-empty
curl -fsSL https://usezombie.sh -o /tmp/install.sh    # HTTP 200, valid TLS, no --insecure
curl -fsSL https://usezombie.sh | head -1             # -> #!/usr/bin/env bash
curl -sSI https://usezombie.sh/install.sh | grep -i content-type   # -> text/x-shellscript
```

## Prerequisite for a meaningful end-to-end install

The script runs `npm install -g @usezombie/zombiectl`. At authoring time npm lagged at `0.3.0` vs the repo's `0.37.0`, and GitHub Releases topped out at `v0.4.0`. Publish a current `@usezombie/zombiectl` to npm before treating a live `curl … | bash` install as the real end-to-end path — that is a separate release task, not part of this wiring.
