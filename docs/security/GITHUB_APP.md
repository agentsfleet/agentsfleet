# GitHub App Security

## Why This Exists

Static PAT usage over-scopes repository access and increases blast radius. GitHub App installation tokens provide least privilege and short lifetime.

## Decisions

1. Use GitHub App installation tokens, not PAT fallback.
2. Tokens are generated per run and not persisted.
3. Credentials originate from environment-backed secrets.

## What This Prevents

1. Repo overreach from long-lived static tokens.
2. Credential reuse across unrelated repositories.
3. Secret leakage from stored runtime token artifacts.

## Required Configuration

1. `GITHUB_APP_ID`
2. `GITHUB_APP_PRIVATE_KEY`
3. callback path configured for installation flow where applicable

## Software Setup Steps

1. Create GitHub App with required repo permissions.
2. Store app ID/private key in environment secret manager.
3. Ensure worker uses installation token exchange flow.
4. Verify token scope and expiration behavior in integration path.

## Verification

1. Token lifetime is short and regenerated as needed.
2. No PAT fallback path is active in hardened mode.
3. PR/push operations succeed only for installed repository scope.
