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
Vault: ZMB_CD_PROD
Item: vercel-bypass-app
Field: credential → paste new value
```

5. Signal agent: "Vercel bypass rotated"

---

## 2.0 Upstash Redis Password Rotation

**Who:** Human (step 1–3) + Agent (step 4)
**Why:** Upstash password appeared in a prior agent conversation session.

### Human Steps

1. Open Upstash console → `helped-hookworm-64126` database → Settings
2. Reset password (Upstash generates a new one)
3. Update 1Password base URL field:

```
Vault: ZMB_CD_DEV
Item: upstash-dev
Field: url → paste new Redis URL (password is embedded)
```

4. Signal agent: "Upstash password rotated"

### Agent Steps

5. Derive `api-url` and `worker-url` from the rotated base `url`:

```bash
BASE_URL=$(op read 'op://ZMB_CD_DEV/upstash-dev/url')
op item edit upstash-dev --vault ZMB_CD_DEV "api-url=${BASE_URL}/0?role=api"
op item edit upstash-dev --vault ZMB_CD_DEV "worker-url=${BASE_URL}/0?role=worker"
```

6. Verify all three fields have the same password:

```bash
extract_pass() { echo "$1" | sed 's|rediss://[^:]*:\([^@]*\)@.*|\1|'; }
BASE=$(op read 'op://ZMB_CD_DEV/upstash-dev/url')
API=$(op read 'op://ZMB_CD_DEV/upstash-dev/api-url')
WORKER=$(op read 'op://ZMB_CD_DEV/upstash-dev/worker-url')
[ "$(extract_pass "$BASE" | shasum)" = "$(extract_pass "$API" | shasum)" ] && \
[ "$(extract_pass "$BASE" | shasum)" = "$(extract_pass "$WORKER" | shasum)" ] && \
echo "ALL FIELDS IN SYNC" || echo "MISMATCH — check manually"
```

---

## 3.0 Agent Pre-Rotation — Capture Old Secret

**Who:** Agent
**Run before:** Human starts §1.0 and §2.0

### Steps

1. Read the current (pre-rotation) bypass secret from vault and store locally for rejection testing:

```bash
OLD_BYPASS="$(op read 'op://ZMB_CD_PROD/vercel-bypass-app/credential')"
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
NEW_SECRET="$(op read 'op://ZMB_CD_PROD/vercel-bypass-app/credential')"
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

6. Run vault sync gate to verify all credentials are consistent:

```bash
./playbooks/gates/m7_002/run.sh
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

3. Run full gate check to confirm rotation is complete:

```bash
./playbooks/gates/m7_002/run.sh
```

---

## 6.0 Exit Criteria

- [ ] Old Vercel bypass secret returns 401/403
- [ ] New Vercel bypass secret returns 200
- [ ] `healthz` + `readyz` green after Upstash password rotation
- [ ] Redis worker `WriteFailed` loop resolved (or confirmed as separate ACL issue)
- [ ] CI logs show `***` for `VERCEL_BYPASS_SECRET`
- [ ] Update evidence doc: mark P0 security items as resolved
- [ ] Gate passes: `./playbooks/gates/m7_002/run.sh` exits 0
