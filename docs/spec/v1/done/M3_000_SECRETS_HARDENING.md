# M3_000: Secrets Hardening — Vault Schema + Envelope Encryption

Date: Mar 3, 2026
Status: ✅ DONE (Mar 05, 2026)
Priority: P0 — security blocker before any customer data
Depends on: M1 control plane operational

---

## Problems

### Problem 1: base64url TEXT encoding is wrong

`schema/001_initial.sql` stores ciphertext as TEXT with comment `base64url(nonce||ct||tag)`. This is a hand-rolled binary-over-text format.

- ~33% size overhead from base64 encoding binary blobs
- Brittle concatenated wire format (`nonce||ct||tag`) — no versioning, no field boundaries
- TEXT column type bypasses Postgres type safety on binary data
- Hand-rolled envelope = hard to audit, easy to misuse

### Problem 2: secrets table is in the public schema

Any DB role that can read `runs` or `specs` can also `SELECT` from `secrets`. Postgres enforces nothing. The separation is application-only. This is wrong.

### Problem 3: GitHub App flow is not documented at implementation level

`docs/ARCHITECTURE.md` describes the user-facing OAuth flow but not what the code does: JWT construction, token exchange, storage path, or token lifecycle.

---

## Solution

### 1. Vault schema — Postgres schema separation with roles

**Migration: `schema/002_vault_schema.sql`**

```sql
-- ── Vault schema ──────────────────────────────────────────────────────────
CREATE SCHEMA IF NOT EXISTS vault;

-- vault_accessor: ONLY vault schema — no access to public
CREATE ROLE vault_accessor;
GRANT USAGE ON SCHEMA vault TO vault_accessor;

-- api_accessor: public schema only — cannot touch vault
CREATE ROLE api_accessor;
GRANT USAGE ON SCHEMA public TO api_accessor;
GRANT SELECT, INSERT, UPDATE ON
    runs, specs, workspaces, tenants, policy_events,
    run_transitions, artifacts, usage_ledger, workspace_memories
TO api_accessor;

-- worker_accessor: public + vault (needs secrets for git auth)
CREATE ROLE worker_accessor;
GRANT api_accessor TO worker_accessor;
GRANT vault_accessor TO worker_accessor;
GRANT SELECT, INSERT, UPDATE ON ALL TABLES IN SCHEMA vault TO worker_accessor;

-- callback_accessor: vault only — used by the GitHub App callback handler
CREATE ROLE callback_accessor;
GRANT vault_accessor TO callback_accessor;
GRANT SELECT, INSERT, UPDATE ON ALL TABLES IN SCHEMA vault TO callback_accessor;

-- Drop and recreate secrets in vault schema (no data yet)
DROP TABLE IF EXISTS public.secrets;
```

**Verification:** `api_accessor` role must get `ERROR: permission denied for schema vault` on any `SELECT` from `vault.secrets`.

### 2. Envelope encryption with BYTEA columns

Replace the single `ciphertext TEXT` column with structured binary columns. No base64 anywhere in the storage path.

**`vault.secrets` table:**

```sql
CREATE TABLE vault.secrets (
    id              BIGSERIAL PRIMARY KEY,
    workspace_id    TEXT    NOT NULL,
    key_name        TEXT    NOT NULL,
    -- Envelope: DEK encrypted with KEK
    kek_version     INTEGER NOT NULL DEFAULT 1,  -- tracks which KEK was used; enables rotation
    encrypted_dek   BYTEA   NOT NULL,            -- AES-256-GCM(DEK, KEK)
    dek_nonce       BYTEA   NOT NULL,            -- 12 bytes, random, for DEK encryption
    dek_tag         BYTEA   NOT NULL,            -- 16-byte GCM auth tag for DEK
    -- Payload: plaintext encrypted with DEK
    nonce           BYTEA   NOT NULL,            -- 12 bytes, random, for payload encryption
    ciphertext      BYTEA   NOT NULL,            -- AES-256-GCM(plaintext, DEK)
    tag             BYTEA   NOT NULL,            -- 16-byte GCM auth tag for payload
    created_at      BIGINT  NOT NULL,
    updated_at      BIGINT  NOT NULL,
    UNIQUE (workspace_id, key_name)
);

CREATE INDEX IF NOT EXISTS idx_vault_secrets_workspace
    ON vault.secrets(workspace_id, key_name);

ALTER TABLE vault.secrets ENABLE ROW LEVEL SECURITY;
```

**Encryption model:**

```
KEK (Key Encryption Key)
  → source: ENCRYPTION_MASTER_KEY env var (64 hex chars = 32 bytes)
  → never stored in DB, loaded at runtime from vault
  → rotated quarterly via kek_version column

DEK (Data Encryption Key)
  → generated fresh per secret: crypto.random_bytes(32)
  → encrypted with KEK → stored as encrypted_dek BYTEA
  → rotating KEK = re-encrypt DEKs only, not all ciphertexts

Payload encryption
  → plaintext encrypted with DEK
  → stored as nonce + ciphertext + tag BYTEA columns
  → nonce is 12 bytes random, never reused
```

**`src/secrets/crypto.zig` — required API:**

```zig
// Store a secret. Generates fresh DEK and nonce per call.
pub fn store(
    conn: *pg.Conn,
    workspace_id: []const u8,
    key_name: []const u8,
    plaintext: []const u8,
    kek: [32]u8,
) !void

// Load and decrypt a secret. Returns owned slice — caller frees.
pub fn load(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    workspace_id: []const u8,
    key_name: []const u8,
    kek: [32]u8,
) ![]const u8

// Parse KEK from ENCRYPTION_MASTER_KEY env var (64 hex chars → 32 bytes).
pub fn loadKek(alloc: std.mem.Allocator) ![32]u8
```

Use `std.crypto.aead.aes_gcm.Aes256Gcm` from the Zig standard library. No external crypto dependency.

### 3. Connection string split

`src/db/pool.zig` reads different env vars depending on binary mode:

| Mode | Env var | Postgres role |
|------|---------|---------------|
| `zombied serve` (API only) | `DATABASE_URL_API` | `api_accessor` |
| `zombied worker` | `DATABASE_URL_WORKER` | `worker_accessor` |
| GitHub callback handler | `DATABASE_URL_CALLBACK` | `callback_accessor` |

If `DATABASE_URL_API` / `DATABASE_URL_WORKER` are not set, fall back to `DATABASE_URL` (single-role local dev).

**`schema/embed.zig`** must embed both migration files:

```zig
pub const initial_sql = @embedFile("001_initial.sql");
pub const vault_sql = @embedFile("002_vault_schema.sql");
```

`src/main.zig` runs both migrations in sequence on startup.

### 4. GitHub App flow — implementation detail in ARCHITECTURE.md

Add sub-section to "Credential Model" in `docs/ARCHITECTURE.md`:

```
#### GitHub App — Implementation Detail

Operator setup (one-time):
  - Register UseZombie as a GitHub App at github.com/settings/apps
  - Callback URL: https://api.usezombie.com/v1/github/callback
  - Permissions: Contents (read/write), Pull Requests (read/write), Metadata (read)
  - Download private key (.pem) → store in CLW_PROD vault as GITHUB_APP_PRIVATE_KEY
  - GITHUB_APP_ID (numeric) → store in CLW_PROD vault

Customer install (per workspace, one-time per repo):
  - zombiectl workspace add <repo_url>
  - UseZombie redirects to:
      github.com/apps/usezombie/installations/new?state=<workspace_id>
  - Customer clicks Authorize in browser
  - GitHub POSTs installation_id to /v1/github/callback?installation_id=X&state=<workspace_id>
  - Handler calls vault.store(workspace_id, "github_app_installation_id", installation_id)
  - Stored in vault.secrets as encrypted BYTEA — no plaintext ever written to DB

Per-run token generation (every run, never stored):
  - Worker calls vault.load(workspace_id, "github_app_installation_id")
  - Constructs GitHub App JWT:
      header: { alg: "RS256", typ: "JWT" }
      payload: { iss: GITHUB_APP_ID, iat: now, exp: now+60 }
      signed with GITHUB_APP_PRIVATE_KEY (RS256)
  - POST https://api.github.com/app/installations/{id}/access_tokens
      Authorization: Bearer <jwt>
  - Response: { token: "ghs_...", expires_at: "<1hr from now>" }
  - Token used for git clone/push/PR creation — discarded after use, never stored
```

---

## Environment Variables

```dotenv
# Split DB roles (production)
DATABASE_URL_API=postgres://api_accessor:<pass>@<host>/usezombiedb
DATABASE_URL_WORKER=postgres://worker_accessor:<pass>@<host>/usezombiedb
DATABASE_URL_CALLBACK=postgres://callback_accessor:<pass>@<host>/usezombiedb

# Master encryption key — 64 hex chars (32 bytes)
ENCRYPTION_MASTER_KEY=<64 hex chars>

# GitHub App (operator credentials — in CLW_PROD vault)
GITHUB_APP_ID=<numeric app id>
GITHUB_APP_PRIVATE_KEY=<contents of .pem file>
GITHUB_CLIENT_ID=<oauth client id>
GITHUB_CLIENT_SECRET=<oauth client secret>
```

**Proton Pass vault entries to create (CLW_PROD):**

```bash
pass-cli item update --vault-name CLW_PROD --item-title ENCRYPTION_MASTER_KEY   --field "password=<64 hex>"
pass-cli item update --vault-name CLW_PROD --item-title GITHUB_APP_ID           --field "password=<id>"
pass-cli item update --vault-name CLW_PROD --item-title GITHUB_APP_PRIVATE_KEY  --field "password=$(cat private-key.pem)"
pass-cli item update --vault-name CLW_PROD --item-title GITHUB_CLIENT_ID        --field "password=<id>"
pass-cli item update --vault-name CLW_PROD --item-title GITHUB_CLIENT_SECRET    --field "password=<secret>"
# Repeat for CLW_DEV with dev app credentials
```

---

## Files to Create or Modify

| File | Action |
|------|--------|
| `schema/002_vault_schema.sql` | Create — vault schema, roles, vault.secrets table |
| `schema/embed.zig` | Modify — embed 002_vault_schema.sql |
| `src/secrets/crypto.zig` | Rewrite — BYTEA envelope encryption, no base64 |
| `src/db/pool.zig` | Modify — split DATABASE_URL_API / DATABASE_URL_WORKER |
| `src/main.zig` | Modify — run vault migration, load KEK, remove GITHUB_PAT remnants |
| `src/http/handler.zig` | Add — GET /v1/github/callback handler |
| `docs/ARCHITECTURE.md` | Modify — add GitHub App implementation detail sub-section |
| `.env.example` | Modify — add DATABASE_URL_API, DATABASE_URL_WORKER, DATABASE_URL_CALLBACK |

---

## Acceptance Criteria

1. `api_accessor` role gets `permission denied for schema vault` on any query to `vault.secrets` — verified with `psql`.
2. `worker_accessor` role can INSERT and SELECT from `vault.secrets`.
3. `store()` writes three BYTEA columns (nonce, ciphertext, tag) + three for encrypted DEK — no TEXT, no base64 anywhere in the row.
4. `load()` returns original plaintext — round-trip test passes.
5. Tampering with `tag` in the DB causes `load()` to return `error.AuthenticationFailed` — GCM tag verification works.
6. `zombied doctor` checks `ENCRYPTION_MASTER_KEY`, `GITHUB_APP_ID`, `GITHUB_APP_PRIVATE_KEY` — all must be non-empty.
7. ARCHITECTURE.md Credential Model section includes the implementation detail sub-section.
8. `kek_version` column is present and defaults to 1 — KEK rotation path is structurally ready even if the rotation command is not yet implemented.

## Out of Scope

1. KEK rotation command (future — `kek_version` column makes it possible when needed).
2. GitHub App JWT signing — that is `M2_009_GITHUB_APP_JWT.md`.
3. Row-level security policies beyond `ENABLE ROW LEVEL SECURITY` — future compliance milestone.
4. External secrets manager (Vault, AWS KMS) — KEK from env is sufficient for M1/M2.
