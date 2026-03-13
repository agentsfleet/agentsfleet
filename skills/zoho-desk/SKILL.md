---
name: zoho-desk
description: Set up Zoho Desk CLI integration from provided credentials and verify operational connectivity.
---

# Zoho Desk Setup

Quick setup for Zoho Desk CLI integration using pre-configured credentials.

## Prerequisites

- Node.js installed (`node --version` to verify)
- Credentials provided by your admin (`.env` file)
- Zoho Desk Organization ID (provided by admin)

## Overview

This skill helps users set up Zoho Desk integration by:

1. Accepting credentials from admin (`.env` content)
2. Creating config directory with correct permissions
3. Installing the Zoho Desk CLI script
4. Asking for Desk Organization ID
5. Saving config securely
6. Verifying connection to Zoho Desk
7. Confirming setup is complete

**For shared service account:** Admin distributes credentials, users just paste and verify.

## Instructions

### 1. Explain What's Happening

Tell the user:
```
I'll help you set up Zoho Desk integration.

You'll need:
- The .env file content from your admin (3 lines + Desk org ID)
- The zoho-desk.json file (if not already configured)

This takes about 1 minute.
```

### 2. Check for Existing Setup

First, check if user already has credentials and Desk config:
```bash
ls -la ~/.config/e2e/agent-profiles/.env 2>/dev/null
ls -la ~/.config/e2e/agent-profiles/zoho-desk.json 2>/dev/null
```

If both files exist, ask: "You already have Zoho Desk configured. Would you like to verify the connection or reconfigure?"

### 3. Create Config Directory and Install Script

```bash
mkdir -p ~/.config/e2e/agent-profiles
chmod 700 ~/.config/e2e/agent-profiles
```

The Zoho Desk CLI script (`zoho-desk.mjs`) is already installed to `~/.config/e2e/agent-profiles/` by `run.sh`. No manual copy needed.

### 4. Ask User to Provide Credentials

Prompt the user:
```
Please paste the contents of your .env file (provided by your admin).
You need the same OAuth credentials used for Zoho Sprint plus the Desk org ID:

ZOHO_CLIENT_ID=...
ZOHO_CLIENT_SECRET=...
ZOHO_DESK_REFRESH_TOKEN=...
ZOHO_DESK_ORG_ID=...
```

Once user provides the content, save it (append if `.env` already exists with Sprint vars):
```bash
cat > ~/.config/e2e/agent-profiles/.env << 'EOF'
[paste user's content here]
EOF
chmod 600 ~/.config/e2e/agent-profiles/.env
```

### 5. Check for Desk Config

Check if `zoho-desk.json` exists:
```bash
ls -la ~/.config/e2e/agent-profiles/zoho-desk.json 2>/dev/null
```

If it doesn't exist, create it with the org ID provided by the user:
```bash
cat > ~/.config/e2e/agent-profiles/zoho-desk.json << 'EOF'
{
  "orgId": "...",
  "baseUrl": "https://desk.zoho.com/api/v1",
  "departmentId": ""
}
EOF
chmod 600 ~/.config/e2e/agent-profiles/zoho-desk.json
```

Replace `"orgId"` value with the `ZOHO_DESK_ORG_ID` from the user's `.env`.

### 6. Verify Connection

Test the setup:
```bash
bun ~/.config/e2e/agent-profiles/zoho-desk.mjs tickets --limit 5
```

**Expected output:**
```json
{
  "tickets": [
    {
      "id": "123456000000100001",
      "subject": "Example ticket",
      "status": "Open"
    }
  ]
}
```

### 7. Confirm Success

Tell the user:
```
✓ Zoho Desk setup complete!

Connection verified - found X ticket(s).

You can now use:
  bun ~/.config/e2e/agent-profiles/zoho-desk.mjs tickets        - List tickets
  bun ~/.config/e2e/agent-profiles/zoho-desk.mjs get <ticketId>  - Get ticket details with threads
  bun ~/.config/e2e/agent-profiles/zoho-desk.mjs pull --output ./desk-export  - Pull all tickets as YAML

Configuration saved to:
  ~/.config/e2e/agent-profiles/.env            (credentials, mode 0600)
  ~/.config/e2e/agent-profiles/zoho-desk.json  (desk config, mode 0600)
```

## Error Handling

### Credentials File Not Found

**Symptoms**:
```
Error: ~/.config/e2e/agent-profiles/.env not found
```

**Solution**: User hasn't pasted credentials yet. Ask them to provide the `.env` content from their admin.

### Missing Org ID

**Symptoms**:
```
Error: ZOHO_DESK_ORG_ID not set
```

**Solution**: The `.env` file is missing `ZOHO_DESK_ORG_ID`. Ask admin for the Desk organization ID and add it to `.env`.

### Connection Verification Fails

**Symptoms**:
```
Token refresh failed: 401 Unauthorized
No access_token in response: { error: 'invalid_code' }
```

**Solutions**:
- Credentials may be incorrect or expired
- Ask admin for fresh credentials
- Ensure you copied all lines from `.env` exactly
- Check file permissions: `chmod 600 ~/.config/e2e/agent-profiles/.env`

### Script Not Found

**Symptoms**:
```
node: cannot find module ~/.config/e2e/agent-profiles/zoho-desk.mjs
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
# Then run zoho-desk setup again with new credentials
```

## Security Notes

**Credential Storage**:
- Credentials stored in `~/.config/e2e/agent-profiles/.env` with mode `0600`
- Contains: client_id, client_secret, refresh_token, desk org ID
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
- Users run zoho-desk setup again with fresh credentials

## For Admins Only

**To generate new credentials** (when rotating or creating initial setup):
1. Go to https://api-console.zoho.com/
2. Create "Self Client" (if not exists)
3. Generate authorization code with scope:
   ```
   ZohoDesk.tickets.READ,ZohoDesk.contacts.READ,ZohoDesk.settings.READ
   ```
4. Exchange code for refresh token (use curl or script)
5. Get the Desk Organization ID from Zoho Desk → Setup → Developer Space → API
6. Create `.env` file with client_id, client_secret, refresh_token, and `ZOHO_DESK_ORG_ID`
7. Distribute securely to team

**Note**: If the team already has Sprint credentials (same OAuth app), only `ZOHO_DESK_ORG_ID` and the Desk scopes need to be added. Ensure the Self Client's scope list includes both Sprint and Desk scopes.

## See Also

- `zoho-setup` — Set up Zoho Sprint integration (shares OAuth credentials)
- Zoho API Console: https://api-console.zoho.com/
- Zoho Desk API docs: https://desk.zoho.com/DeskAPIDocument
