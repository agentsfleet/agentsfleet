# Security Posture & Cryptographic Risk Assessment

**Date:** Mar 21, 2026
**Status:** Living document — update on infrastructure or algorithm changes

---

## Quantum Attack Timeline

| Source | Estimate | Qubits needed |
|--------|----------|---------------|
| IBM / Google roadmap | 2035–2040 earliest | ~4,000+ logical qubits |
| Gidney & Ekera (2021) | ~8 hours once hardware exists | 2,330 logical qubits (~20M physical) |
| NIST PQC report | Not before 2030, likely later | — |
| BSI (German fed) | Plan for post-2030 | — |

Today: ~1,200 physical qubits (IBM Condor). Need ~20M. 4+ orders of magnitude away.

Once hardware exists, Shor's algorithm breaks P-256 / Ed25519 in **~8 hours** and RSA 2048 in **~4 hours**.

---

## Risk by Asset

| Asset | Algorithm | Lifespan | PQC risk |
|-------|-----------|----------|----------|
| Clerk JWTs | ES256 (P-256) | 60 seconds | **None** — expires before attack completes |
| TLS key exchange | X25519Kyber768 | Per-session | **Protected** — Cloudflare ships PQC |
| TLS certificates | ECDSA P-256 | 90 days (Let's Encrypt) | **Low** — CA/browser ecosystem upgrade, not ours |
| Tailscale WireGuard | Curve25519 | Per-session, rotated | **Low** — ephemeral keys |
| Encryption KEK | AES-256-GCM | Long-lived | **Safe** — Grover's gives 128-bit, infeasible |
| Per-secret DEK | AES-256-GCM | Per-secret | **Safe** — random per encrypt, wrapped by KEK |
| SSH worker keys | Ed25519 | Indefinite | **Real risk** — harvest now, decrypt later |
| GitHub App PEM | RSA 2048 | Long-lived | **Medium** — Shor's in ~4 hrs, rotate annually |

**JWTs are not at risk.** Attacker intercepts JWT → extracts public key from JWKS → runs Shor's (~8 hrs) → forges new JWT. By step 3, token is expired. OIDC abstraction (`OIDC_PROVIDER`, `OIDC_JWKS_URL`) decouples us from Clerk's signing algorithm — when they upgrade, we get new keys via JWKS. No code change.

**AES-256 is not at risk.** Grover's reduces to ~128-bit equivalent — still infeasible. KEK versioning supports rotation (KEK_VERSION 1 → 2). DEKs are random per secret.

**TLS is mostly protected.** Cloudflare Tunnel negotiates X25519Kyber768 hybrid PQC. Certificate forgery requires breaking ECDSA within 90-day rotation window. Certificate Transparency detects forged certs. No action — this is a CA/browser upgrade.

---

## SSH Worker Keys — Harvest Now, Decrypt Later

The real exposure. Ed25519 keys are long-lived, no rotation policy, classically signed.

**Threat:**
1. Attacker records Tailscale SSH traffic today
2. Stores it
3. In 2035, runs Shor's on captured Ed25519 public key (~8 hrs)
4. Derives private key, decrypts all stored sessions

**Current mitigations:** Tailscale WireGuard (ephemeral per-session keys), no public SSH port, bare-metal nodes (not multi-tenant).

### Agent: autorotate-ssh (quarterly)

For each worker node (`zombie-{env}-worker-{name}`):

1. Generate new Ed25519 key pair: `ssh-keygen -t ed25519 -f /tmp/rotate -N ""`
2. SSH to node using current key: `op read "op://{vault}/{node}/ssh-private-key"`
3. Append new public key to `~/.ssh/authorized_keys`
4. Verify SSH connectivity using new key
5. Remove old public key from `authorized_keys`
6. Update vault: `op item edit {node} ssh-private-key="$(cat /tmp/rotate)" --vault {vault}`
7. Verify SSH using vault-sourced key (round-trip)
8. Open PR with evidence: rotation date, node name, key fingerprint (public only)

**Rollback:** If step 4 fails, old key is still in `authorized_keys` and vault. Remove new public key, abort. No downtime.

| Environment | Vault | Items |
|-------------|-------|-------|
| DEV | `ZMB_CD_DEV` | `zombie-dev-worker-ant/ssh-private-key` |
| PROD | `ZMB_CD_PROD` | `zombie-prod-worker-ant/ssh-private-key`, `zombie-prod-worker-bird/ssh-private-key` |

**PQC upgrade path:** OpenSSH 9.0+ supports `sntrup761x25519-sha512` hybrid KEX. OpenSSH 9.9+ supports ML-DSA host keys (`-t ml-dsa-87`). Enable on workers when Debian stable ships these versions.

---

## GitHub App PEM — Rotate Annually

RSA 2048, long-lived, breakable by Shor's in ~4 hrs. Installation tokens are short-lived (1 hour) — limits blast radius. GitHub does not offer PQC key types yet.

### Agent: autorotate-github-app (annually)

1. Read current App ID: `op read "op://ZMB_CD_PROD/github-app/app-id"`
2. Generate new private key via GitHub UI (API support for key generation is limited)
3. Update vault:
   - `op item edit github-app private-key="$(cat new-key.pem)" --vault ZMB_CD_PROD`
   - `op item edit github-app private-key="$(cat new-key.pem)" --vault ZMB_CD_DEV`
4. Fly secrets sync happens on next deploy (CI `--stage` step), or trigger manually
5. Verify: `zombied doctor` reports GitHub App connectivity OK
6. Revoke old key in GitHub UI
7. Open PR with evidence: rotation date, new key fingerprint, doctor output

| Vault | Item | Fields |
|-------|------|--------|
| `ZMB_CD_PROD` | `github-app` | `app-id`, `private-key` |
| `ZMB_CD_DEV` | `github-app` | `app-id`, `private-key` |

---

## Action Items

| Priority | Action | Owner | Target |
|----------|--------|-------|--------|
| P3 | Quarterly SSH key rotation | Agent: `autorotate-ssh` | Q2 2026 |
| P3 | Enable PQC KEX on workers | Agent: when Debian ships OpenSSH 9.x | When available |
| P3 | Annual GitHub App PEM rotation | Agent: `autorotate-github-app` | Mar 2027 |
| P4 | PQC SSH host keys (ML-DSA) | Agent: when OpenSSH 9.9+ in Debian | 2027+ |
| — | Monitor JOSE/JWA ML-DSA for JWT signing | Watch IETF draft | No action until finalized |
