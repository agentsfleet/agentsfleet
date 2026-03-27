# M7_002: Playbook — DEV Credential Rotation

**Milestone:** M7
**Workstream:** 002
**Date:** Mar 27, 2026
**Owner:** Human (steps 1–2), Agent (steps 3–4)
**Prerequisite:** Credential exposure identified in M7_001 acceptance session
**Trigger:** Run whenever a DEV credential is exposed in logs, chat, or commits

---

## 1.0 Vercel Bypass Secret Rotation

**Who:** Human
**Why:** `vercel-bypass-app` was printed in CI run 23629344140 logs before `::add-mask::` was added.

### Steps

1. Open Vercel dashboard → `usezombie-app` project → Settings → Deployment Protection
2. Regenerate the bypass secret (Vercel creates a new random value)
3. Copy the new value
4. Update 1Password:

```
Vault: ZOMBIE_PROD
Item: vercel-bypass-app
Field: credential → paste new value
```

5. Signal agent: "Vercel bypass rotated"

---

## 2.0 Upstash Redis Password Rotation

**Who:** Human
**Why:** Upstash password appeared in a prior agent conversation session.

### Steps

1. Open Upstash console → `helped-hookworm-64126` database → Settings
2. Reset password (Upstash generates a new one)
3. Copy the new Redis URL (password is embedded in the URL)
4. Update 1Password:

```
Vault: ZOMBIE_DEV
Item: upstash-dev
Fields:
  api-url → new URL with updated password
  worker-url → new URL with updated password
```

5. Signal agent: "Upstash password rotated"

---

## 3.0 Agent Pre-Rotation — Capture Old Secret

**Who:** Agent
**Run before:** Human starts §1.0 and §2.0

### Steps

1. Read the current (pre-rotation) bypass secret from vault and store locally for rejection testing:

```bash
OLD_BYPASS="$(op read 'op://ZOMBIE_PROD/vercel-bypass-app/credential')"
echo "$OLD_BYPASS" > /tmp/old-bypass-secret.txt
echo "Old bypass secret saved for post-rotation rejection test"
```

2. Signal human: "Pre-rotation snapshot captured. Proceed with §1.0 and §2.0."

---

## 4.0 Agent Verification — Fly Secrets Sync

**Who:** Agent
**Depends on:** §1.0, §2.0, and §3.0 complete

### Steps

1. Trigger a fresh deploy to pick up new vault values:

```bash
gh workflow run deploy-dev.yml
```

The `deploy-fly-dev` job reads secrets from 1Password and runs `flyctl secrets set`, so a redeploy automatically syncs the new Upstash password to Fly.

2. Wait for pipeline to complete:

```bash
gh run list --workflow "deploy (dev)" --limit 1
gh run view <run-id> --json conclusion
```

3. Verify API is healthy with the new credentials:

```bash
curl -sf https://api-dev.usezombie.com/healthz
curl -sf https://api-dev.usezombie.com/readyz | jq -e '.ready == true'
```

4. Verify Vercel bypass works with the new (rotated) secret from vault:

```bash
NEW_SECRET="$(op read 'op://ZOMBIE_PROD/vercel-bypass-app/credential')"
curl -sf -o /dev/null -w '%{http_code}' \
  -H "x-vercel-protection-bypass: $NEW_SECRET" \
  "https://usezombie-app.vercel.app/sign-in"
# Expected: 200
```

5. Verify old bypass secret (captured in §3.0) no longer works:

```bash
OLD_BYPASS="$(cat /tmp/old-bypass-secret.txt)"
curl -sf -o /dev/null -w '%{http_code}' \
  -H "x-vercel-protection-bypass: $OLD_BYPASS" \
  "https://usezombie-app.vercel.app/sign-in"
# Expected: 401 or 403
rm /tmp/old-bypass-secret.txt
```

---

## 5.0 Agent Verification — CI Masking

**Who:** Agent

1. Check the new deploy-dev run logs to confirm `VERCEL_BYPASS_SECRET` is masked (shows `***`):

```bash
gh run view <run-id> --log --job <qa-dev-job-id> | grep -i "VERCEL_BYPASS"
# Should show *** not the actual value
```

2. If the secret still appears in plaintext, the `::add-mask::` step needs debugging.

---

## 6.0 Exit Criteria

- [ ] Old Vercel bypass secret returns 401/403
- [ ] New Vercel bypass secret returns 200
- [ ] `healthz` + `readyz` green after Upstash password rotation
- [ ] Redis worker `WriteFailed` loop resolved (or confirmed as separate ACL issue)
- [ ] CI logs show `***` for `VERCEL_BYPASS_SECRET`
- [ ] Update evidence doc: mark P0 security items as resolved
