---
name: zoho-sprint
description: Set up Zoho Sprint CLI integration from provided credentials and verify operational connectivity.
---

# Zoho Setup

Quick setup for Zoho Sprint CLI integration using pre-configured credentials.

## Prerequisites

- Node.js installed (`node --version` to verify)
- Credentials provided by your admin (`.env` file)

## Overview

This skill helps users set up Zoho Sprint integration by:

1. Accepting credentials from admin (`.env` content)
2. Creating config directory with correct permissions
3. Saving credentials securely
4. Verifying connection to Zoho Sprint
5. Confirming setup is complete

**For shared service account:** Admin distributes credentials, users just paste and verify.

## Instructions

### 1. Explain What's Happening

Tell the user:
```
I'll help you set up Zoho Sprint integration.

You'll need:
- The .env file content from your admin (3 lines)
- The zoho.json file (if not already configured)

This takes about 1 minute.
```

### 2. Check for Existing Setup

First, check if user already has credentials:
```bash
ls -la ~/.config/e2e/agent-profiles/.env 2>/dev/null
```

If file exists, ask: "You already have Zoho credentials configured. Would you like to verify the connection or reconfigure?"

### 3. Create Config Directory and Install Script

```bash
mkdir -p ~/.config/e2e/agent-profiles
chmod 700 ~/.config/e2e/agent-profiles
```

The Zoho Sprint CLI script (`zoho-sprint.mjs`) is already installed to `~/.config/e2e/agent-profiles/` by `run.sh`. No manual copy needed.

### 4. Ask User to Provide Credentials

Prompt the user:
```
Please paste the contents of your .env file (3 lines provided by your admin):

ZOHO_CLIENT_ID=...
ZOHO_CLIENT_SECRET=...
ZOHO_SPRINT_REFRESH_TOKEN=...
```

Once user provides the content, save it:
```bash
cat > ~/.config/e2e/agent-profiles/.env << 'EOF'
[paste user's content here]
EOF
chmod 600 ~/.config/e2e/agent-profiles/.env
```

### 5. Check for Project Config

Check if `zoho.json` exists:
```bash
ls -la ~/.config/e2e/agent-profiles/zoho.json 2>/dev/null
```

If it doesn't exist, ask user to provide it or use a default template.

### 6. Verify Connection

Test the setup:
```bash
bun ~/.config/e2e/agent-profiles/zoho-sprint.mjs sprints
```

**Expected output:**
```json
{
  "sprints": [
    {
      "id": "8253000003224115",
      "name": "Q1 (January - March) 2026",
      "status": "2"
    }
  ]
}
```

### 7. Confirm Success

Tell the user:
```
✓ Zoho Sprint setup complete!

Connection verified - found X sprint(s) in project.

You can now use:
  `bun ~/.config/e2e/agent-profiles/zoho-sprint.mjs items` - List Zoho Sprint items
  `pull-tickets.md` workflow - Pull and process ticket queues

Configuration saved to:
  ~/.config/e2e/agent-profiles/.env (credentials, mode 0600)
  ~/.config/e2e/agent-profiles/zoho.json (project config)

Available commands:
   bun ~/.config/e2e/agent-profiles/zoho-sprint.mjs sprints  - List sprints
   bun ~/.config/e2e/agent-profiles/zoho-sprint.mjs items    - List items
   bun ~/.config/e2e/agent-profiles/zoho-sprint.mjs projects - List projects
```

### 8. Additional Information

**Switch projects** (if multiple configured):
```bash
bun ~/.config/e2e/agent-profiles/zoho-sprint.mjs use <project-name>
```

**View available projects**:
```bash
bun ~/.config/e2e/agent-profiles/zoho-sprint.mjs projects
```

### 7. Multi-Project Setup (Optional)

If user has multiple projects, help them add to config.

Edit `~/.config/e2e/agent-profiles/zoho.json`:
```json
{
  "current": "sre",
  "projects": {
    "sre": {
      "teamId": "667504989",
      "projectId": "8253000000450103",
      "workspaceName": "yourworkspace",
      "projectNo": "14"
    },
    "discovery": {
      "teamId": "667504989",
      "projectId": "8253000000154001",
      "workspaceName": "yourworkspace",
      "projectNo": "11"
    }
  }
}
```

Switch projects with:
```bash
bun ~/.config/e2e/agent-profiles/zoho-sprint.mjs use discovery
```

## Error Handling

### Credentials File Not Found

**Symptoms**:
```
Error: ~/.config/e2e/agent-profiles/.env not found
```

**Solution**: User hasn't pasted credentials yet. Ask them to provide the `.env` content from their admin.

### Connection Verification Fails

**Symptoms**:
```
Token refresh failed: 401 Unauthorized
No access_token in response: { error: 'invalid_code' }
```

**Solutions**:
- Credentials may be incorrect or expired
- Ask admin for fresh credentials
- Ensure you copied all 3 lines from `.env` exactly
- Check file permissions: `chmod 600 ~/.config/e2e/agent-profiles/.env`

### Script Not Found

**Symptoms**:
```
node: cannot find module ~/.config/e2e/agent-profiles/zoho-sprint.mjs
```

**Solution**:
 Script may not be installed. Re-run `./scripts/run.sh` from the repo root to deploy it.

### Permission Denied

**Symptoms**:
```
Error: EACCES: permission denied, open '~/.config/e2e/agent-profiles/.env'
```

**Solution**:
```bash
# Ensure directory exists and has correct permissions
mkdir -p ~/.config/e2e/agent-profiles
chmod 700 ~/.config/e2e/agent-profiles
```

### Token Expired After Working

**Symptoms**:
- Setup worked initially
- Now getting "401 Unauthorized" errors

**Solution**:
The refresh token expired (typically after 60-90 days). Contact admin for fresh credentials:
```bash
rm ~/.config/e2e/agent-profiles/.env
rm ~/.config/e2e/agent-profiles/.zoho-token-cache.json
# Then run /zoho-setup again with new credentials
```

## Security Notes

**Credential Storage**:
- Credentials stored in `~/.config/e2e/agent-profiles/.env` with mode `0600`
- Contains: client_id, client_secret, refresh_token
- **Never commit to git** or share via insecure channels

**Shared Service Account**:
- Credentials are shared across team
- All Zoho actions appear under the shared account
- No per-user attribution of changes

**Token Lifecycle**:
- **Access token**: Expires every 1 hour, auto-refreshed by script
- **Refresh token**: Long-lived (~60-90 days), stored in `.env`
- **Authorization code**: Expires in 10 minutes, only used once (not needed after initial setup)

**Credential Distribution**:
- Admin should distribute via secure channel (1Password, encrypted email)
- Recipients paste into local `.env` file only
- Never send via Slack, unencrypted email, or public channels

**Rotation**:
- If compromised, admin generates new credentials and redistributes
- Users run `/zoho-setup` again with fresh credentials

## For Admins Only

**To generate new credentials** (when rotating or creating initial setup):
1. Go to https://api-console.zoho.com/
2. Create "Self Client" (if not exists)
3. Generate authorization code with scope:
   ```
   ZohoSprints.teams.READ,ZohoSprints.projects.READ,ZohoSprints.sprints.READ,
   ZohoSprints.items.READ,ZohoSprints.items.CREATE,ZohoSprints.items.UPDATE,
   ZohoSprints.epic.READ,ZohoSprints.epic.CREATE,ZohoSprints.epic.UPDATE
   ```
4. Exchange code for refresh token (use curl or script)
5. Create `.env` file with client_id, client_secret, refresh_token
6. Distribute securely to team

## See Also

- `pull-tickets.md` — Pull and process ticket workflow after setup
- Zoho API Console: https://api-console.zoho.com/
