---
type: how-to
audience: operator
verified: 2026-11-08
product_version: 0.75.0
executable: false
---

# Deploy the `agentsfleet.dev` installer

## What this does

Vercel serves `ui/agentsfleet.dev/dist/` as a static site.
A Pull Request (PR) creates a preview, and a merge to `main` deploys production.
The repository has no installer deploy script and no Vercel credential in GitHub Actions.
First-install setup belongs in founding step 01.
`dist/vercel.json` rewrites `/` to `/install.sh` and sets `Content-Type: text/x-shellscript` and `Cache-Control: public, max-age=300`.

## Before you begin

The Vercel project must hold these settings; 🤠 Indy repairs project or domain ownership.

| Setting | Value |
|---|---|
| Project | `agentsfleet-agents-dev` |
| Production branch | `main` |
| Framework | None |
| Build command | Empty |
| Root directory | `ui/agentsfleet.dev/dist` |
| Production domain | `agentsfleet.dev` |

## Steps

1. Change `ui/agentsfleet.dev/dist/install.sh` or `vercel.json`.
2. Run the repository verification commands.
3. Open a PR and check its Vercel preview.
   If the preview is missing or the domain is detached, stop; 🤠 Indy repairs it.
4. Merge once the preview and checks are green.

## Verify it works

🦉 The agent runs these read-only checks.

```bash
dig +short A agentsfleet.dev
curl -fsSL https://agentsfleet.dev -o /tmp/agentsfleet-install.sh
head -1 /tmp/agentsfleet-install.sh
curl -sSI https://agentsfleet.dev/install.sh | grep -i content-type
```

```text
<varies: one or more IP addresses>
#!/usr/bin/env bash
content-type: text/x-shellscript; charset=utf-8
```

The `curl` download must succeed over Transport Layer Security (TLS).
A complete install also needs the published `@agentsfleet/cli` version to match `VERSION`; the production release route checks that.
