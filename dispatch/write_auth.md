# write_auth.md — auth-flow surface dispatch (LATENT façade, fully delegated)

This is the prose the AGENT reads **before touching any auth-flow, token-minting,
or credential-typed code**. It is **fully ⚪ delegated**: the canonical rule doc
`AUTH.md` lives in the **product repo**, not in dotfiles, so dotfiles cannot
mechanically prove its coverage. This façade carries only the trigger + the routing
+ the cross-cutting auth invariants that hold regardless of repo.

**Signal legend:**

- ⚪ delegated — the auth rules + their checks live and run in the product repo
  (agentsfleet `docs/AUTH.md`). Dotfiles routes to them and enforces the
  always-forbidden credential rules (below) that apply everywhere.

## Trigger — read the product repo's `docs/AUTH.md` before

- Editing `src/auth/**`, `ui/packages/app/lib/auth/**`, token-minting handlers,
  or any credential-typed spec dimension.

**Override:** none — auth is a security boundary.

## Always-forbidden (these hold in dotfiles too, per AGENTS.md Hard Safety)

- **Plaintext secrets in entity tables** — store a vault `key_name` ref, resolve
  via `crypto_store.load()` at runtime.
- **Resolving/printing credentials** — never `op read 'op://...'` into logs or
  output at runtime; never paste a resolved secret.
- **Static credential strings in SQL schema** — no `DEFAULT 'value'` /
  `CHECK (col IN (...))`; enforce in app via named constants.

## What it routes to

`docs/AUTH.md` **in the product repo** — token lifecycle, session shape, the
auth middleware chain, and credential storage rules. Read it first for any
auth-flow change; this façade exists so the dispatch table has a home for the
trigger even though the rule body ships with the product, not dotfiles.
