# `ui/usezombie.sh/`

The one-URL installer for `zombiectl` + the platform-ops skill, served at `https://usezombie.sh`.

```
curl -fsSL https://usezombie.sh | bash
```

This is a static site — no build step. `dist/` is committed and served by Vercel as-is.

## Layout

| Path | Purpose | Served? |
|------|---------|---------|
| `dist/install.sh` | POSIX (Linux/macOS) installer. npm-only — see below. | yes (at `/` and `/install.sh`) |
| `dist/vercel.json` | Rewrites the bare root to the installer (`/  →  /install.sh`), sets the shell content-type + 5-min cache. | config |
| `install_test.sh` | Hermetic black-box smoke tests for `dist/install.sh`. | no |
| `README.md` | This file. | no |

The Windows PowerShell installer (`install.ps1`) is a separate follow-up — not here yet.

## What `dist/install.sh` does

1. Requires Node.js (`node` + `npm`) — `zombiectl` is an npm-distributed Node CLI (`bin: ./dist/bin/zombiectl.js`), so there is no standalone binary to fall back to. Missing Node exits `5` with an install link.
2. Detects the agent host on `$PATH` (`claude` / `amp` / `codex` / `opencode`); ambiguous (more than one) exits `4` unless `USEZOMBIE_HOST` is set.
3. `npm install -g --prefix "$USEZOMBIE_INSTALL" @usezombie/zombiectl[@version]` (default prefix `~/.usezombie`), then adds `<prefix>/bin` to PATH.
4. `npx --yes skills add usezombie/skills --host=<detected>`.
5. Prints the next command: `/usezombie-install-platform-ops`.

### Environment / flags

| Knob | Effect |
|------|--------|
| `USEZOMBIE_INSTALL` | Install prefix (default `~/.usezombie`). Rejected if it contains quotes/backticks/`$`/newline. |
| `USEZOMBIE_HOST` | Force the agent host, skip detection. |
| `bash -s v0.37.0` | Pin a CLI version (`@usezombie/zombiectl@0.37.0`). |
| `bash -s -- --force` | Reinstall without the upgrade prompt. |

### Exit codes

`0` ok · `1` network/DNS · `2` npm install failed · `3` prefix not writable · `4` host ambiguous · `5` Node missing.

## Testing locally

```
shellcheck -s bash dist/install.sh install_test.sh   # zero warnings
bash install_test.sh                                  # hermetic — fakes npm/npx/node, no real install
```

`install_test.sh` builds a sandbox PATH of real coreutils plus per-scenario fakes; no real npm or
network is ever touched.

## Deploying

The served bytes are `dist/`, deployed by a **git-connected Vercel project** (`usezombie-agents-sh`,
framework `None`, root directory `ui/usezombie.sh/dist`) — the same pattern as `usezombie-website`.
There is no deploy workflow and no CI credentials; Vercel's GitHub integration handles it:

- Open a PR touching `ui/usezombie.sh/**` → Vercel auto-deploys a **preview** and comments the URL.
- Merge to `main` → Vercel deploys **production** → `https://usezombie.sh`.

`dist/vercel.json` carries the serving config Vercel reads (the `/ → /install.sh` rewrite, the
`text/x-shellscript` content-type, and the 5-minute cache) — Vercel does **not** read Cloudflare
`_redirects`/`_headers`. The `lint-usezombie-sh` CI job (shellcheck + `install_test.sh`) gates the
merge, so a broken installer can't reach `main`. The 5-minute `Cache-Control` propagates a bump
globally within minutes. One-time Vercel provisioning (project + custom domain) lives in
[`playbooks/014_usezombie_sh_deploy/`](../../playbooks/014_usezombie_sh_deploy/).
