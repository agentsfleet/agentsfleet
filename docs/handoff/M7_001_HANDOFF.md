# Handoff — M7_001 DEV Acceptance Gate

Date: Mar 26, 2026

## Scope/Status

Session root-caused DEV API 502 (IPv4-only bind on Fly IPv6 6PN), fixed it, expanded into error code refactor, test coverage, Greptile review fixes, and migration planning.

- ✅ Dual-stack HTTP fix (`::` binding, `[:0]const u8` for C FFI safety)
- ✅ 55 new tests (RESP protocol, queue constants, backoff, worker state, ServerConfig)
- ✅ `logErrWithHint()` — git-style actionable errors with docs URLs
- ✅ 14 hardcoded error code strings → `error_codes.*` constants
- ✅ `ERROR_DOCS_BASE` corrected to `https://docs.usezombie.com/error-codes#`
- ✅ Greptile P1/P2 all resolved (`loadProfile`, `ERR_STARTUP_REDIS_GROUP`, `[:0]const u8`, `IPV6_V6ONLY`)
- ✅ Test env var rename: `HANDLER_DB_TEST_URL` → `TEST_DATABASE_URL`, `REDIS_TLS_TEST_URL` → `TEST_REDIS_TLS_URL`
- ✅ Redis TLS cert fix (CN=localhost + SANs, CA cert extraction for macOS)
- ✅ Specs: M7_004 (config/playbook alignment), M7_005 (httpz migration)
- ✅ `.gitconfig` per-repo author: usezombie/indykish → `nkishore@megam.io`
- ⏳ M7_001 §1.4/1.5 — healthz/readyz via tunnel (needs deploy)
- ⏳ M7_001 §3.0–8.0 — API health, UI smoke, Playwright QA, CLI acceptance, evidence

## Working Tree

```
## m7/001-dev-acceptance-gate...origin/m7/001-dev-acceptance-gate
```

- Clean, all pushed. 11 commits ahead of main.

## Branch/PR

- Branch: `m7/001-dev-acceptance-gate`
- PR: usezombie/usezombie#91
- Prior PR: #90 (merged — dual-stack fix + error hints + 55 tests)
- Dotfiles PR: indykish/dotfiles#10 (gitconfig per-repo author)
- CI on #91: `lint` ✅, `gitleaks` ✅, `memleak` ✅, `qa-smoke` ✅, `test-integration` ✅, `test` in progress, `qa` in progress

## Running Processes

- Docker Compose: `postgres` (healthy), `redis` (healthy) — `docker compose ps`
- No tmux sessions
- Stop with: `docker compose down`

## Tests/Checks

- ✅ `make lint` — 0 errors, 0 warnings, ZLint 203 files
- ✅ `make test` — 452 passed, 94 skipped, 0 failed
- ✅ `make test-integration` — DB + Redis TLS green
- ✅ `make check-pg-drain` — 201 files
- ✅ `gitleaks detect` — no leaks
- ✅ CI: lint, gitleaks, memleak, qa-smoke, test-integration all green

## Next Steps

1. Wait for CI `test` + `qa` to go green on #91, then merge
2. **Rotate Upstash Redis password** — appeared in conversation, rotate via Upstash dashboard + `fly secrets set`
3. **Enable Upstash IP allowlist** — restrict to Fly `iad` egress IPs
4. Deploy to Fly DEV: `fly deploy --app zombied-dev --image ghcr.io/usezombie/zombied:dev-latest`
5. Verify `curl -sf https://api-dev.usezombie.com/healthz` returns 200 (M7_001 §1.4)
6. Verify `curl -sf https://api-dev.usezombie.com/readyz | jq '.ready'` returns true (M7_001 §1.5)
7. Investigate Redis `WriteFailed` on DEV worker — Upstash ACL or connectivity
8. Run `zombied doctor` against DEV (M7_001 §3.3)
9. UI smoke: Vercel preview URLs for app + website (M7_001 §4.0)
10. CLI acceptance: `zombiectl login → workspace add → specs sync → run → runs list` (M7_001 §6.0)
11. Capture evidence in `docs/evidence/M7_001_DEV_ACCEPTANCE_EVIDENCE.md` (M7_001 §7.0)
12. Start M7_004 (config/playbook alignment) once M7_001 clears
13. Merge indykish/dotfiles#10 (gitconfig per-repo author)

## Risks/Gotchas

- **Upstash Redis is public** — `helped-hookworm-64126.upstash.io:6379` has password-only auth, no IP allowlist. Enable allowlist ASAP.
- **Password rotation** — Upstash password was exposed in this session. Rotate before next deploy.
- **Redis worker `WriteFailed` loop** — `zombied serve` on DEV continuously fails `xreadgroup`/`xautoclaim`. HTTP still works. Likely Upstash ACL issue — worker needs XREADGROUP/XAUTOCLAIM/XACK permissions.
- **`agents_test.runByRole` flaky** — seed-dependent, passes in isolation. Collateral failure when Redis TLS test crashes the process. Not a real bug.
- **`REDIS_READY_TEST_URL`** — dead code in `redis_test.zig`. Queued for removal in M7_004 §3.3.
- **Executor build boundary** — `src/executor/` can't import `src/errors/codes.zig`. Constants duplicated with comment pointing to canonical source.
- **macOS vs Linux TLS** — Zig TLS on macOS needs explicit CA cert for self-signed Redis. Makefile extracts it via `docker compose cp`. CI (Linux) works without it but the Makefile handles both.
