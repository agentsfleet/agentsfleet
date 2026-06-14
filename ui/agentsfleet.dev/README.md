# `ui/agentsfleet.dev/`

The one-URL installer for `agentsfleet` + the platform-ops skill, served at `https://agentsfleet.dev`.

```
curl -fsSL https://agentsfleet.dev | bash
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

1. Requires Node.js (`node` + `npm`) — `agentsfleet` is an npm-distributed Node CLI (`bin: ./dist/bin/agentsfleet.js`), so there is no standalone binary to fall back to. Missing Node exits `5` with an install link.
2. Detects the agent host on `$PATH` (`claude` / `amp` / `codex` / `opencode`); ambiguous (more than one) exits `4` unless `AGENTSFLEET_HOST` is set.
3. `npm install -g --prefix "$AGENTSFLEET_INSTALL" @agentsfleet/cli[@version]` (default prefix `~/.agentsfleet`), then adds `<prefix>/bin` to PATH.
4. `npx --yes skills add agentsfleet/skills --host=<detected>`.
5. Prints the next command: `/agentsfleet-install-platform-ops`.

### Environment / flags

| Knob | Effect |
|------|--------|
| `AGENTSFLEET_INSTALL` | Install prefix (default `~/.agentsfleet`). Rejected if it contains quotes/backticks/`$`/newline. |
| `AGENTSFLEET_HOST` | Force the agent host, skip detection. |
| `bash -s v0.37.0` | Pin a CLI version (`@agentsfleet/cli@0.37.0`). |
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
framework `None`, root directory `ui/agentsfleet.dev/dist`) — the same pattern as `usezombie-website`.
There is no deploy workflow and no CI credentials; Vercel's GitHub integration handles it:

- Open a PR touching `ui/agentsfleet.dev/**` → Vercel auto-deploys a **preview** and comments the URL.
- Merge to `main` → Vercel deploys **production** → `https://agentsfleet.dev`.

`dist/vercel.json` carries the serving config Vercel reads (the `/ → /install.sh` rewrite, the
`text/x-shellscript` content-type, and the 5-minute cache) — Vercel does **not** read Cloudflare
`_redirects`/`_headers`. The `lint-usezombie-sh` CI job (shellcheck + `install_test.sh`) gates the
merge, so a broken installer can't reach `main`. The 5-minute `Cache-Control` propagates a bump
globally within minutes. One-time Vercel provisioning (project + custom domain) lives in
[`playbooks/operations/installer_deploy/`](../../playbooks/operations/installer_deploy/).
