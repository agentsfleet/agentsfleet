# `agentsfleet.dev` Installer Deployment

**Owners:** Vercel deploys; 🦉 Orly verifies; 🤠 Indy repairs project or domain
ownership when required.
**Scope:** routine installer changes. First-install setup belongs in founding
step 01.

Vercel serves `ui/agentsfleet.dev/dist/` as a static site. The repository has
no installer deploy script and no Vercel credential in GitHub Actions. A Pull
Request (PR) creates a preview; merging `main` creates the production deploy.

The required Vercel project settings are:

| Setting | Value |
|---|---|
| Project | `agentsfleet-agents-dev` |
| Production branch | `main` |
| Framework | None |
| Build command | Empty |
| Root directory | `ui/agentsfleet.dev/dist` |
| Production domain | `agentsfleet.dev` |

`dist/vercel.json` rewrites `/` to `/install.sh`, sets the shell-script content
type, and sets the cache policy.

## Deploy

1. Change `ui/agentsfleet.dev/dist/install.sh` or `vercel.json`.
2. Run the installer checks through the repository verification commands.
3. Open a PR and verify its Vercel preview.
4. Merge only after the preview and repository checks are green. Vercel then
   deploys `main` to production.

If the preview is absent or the production domain is not attached to the
project above, stop. 🤠 Indy must repair the Vercel project or domain before the
deployment can be accepted.

## Verify

These commands are read-only:

```bash
dig +short A agentsfleet.dev
curl -fsSL https://agentsfleet.dev -o /tmp/agentsfleet-install.sh
head -1 /tmp/agentsfleet-install.sh
curl -sSI https://agentsfleet.dev/install.sh | grep -i content-type
```

Acceptance requires a non-empty DNS answer, successful Transport Layer
Security (TLS), a `#!/usr/bin/env bash` first line, and the shell-script content
type. A complete install also requires the published `@agentsfleet/cli` version
to match `VERSION`; the production release route owns that check.
