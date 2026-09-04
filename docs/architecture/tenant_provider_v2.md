# Tenant provider activation — the v2 plan

> Parent: [`README.md`](./README.md) · Concept reference for the surface today:
> [`billing_and_provider_keys.md`](./billing_and_provider_keys.md) §1, §8

What `PUT /v1/tenants/me/provider` costs today, why, and the three schema
changes that delete those costs instead of optimizing them. Written during the
Rust cutover (M181), when the activation path was ported at parity; every
workaround the port carries cites this file as its expiry date.

## The principle

The activation transaction runs seven round trips and one decrypt, and that is
the FLOOR for the current schema — each remaining statement compensates for a
named piece of schema debt. Query tuning below that floor is polishing the
workaround. The v2 items each remove a debt, and with it a class of statements,
locks, or failure modes; together they reduce activation to two statements with
no explicit locks and no decrypt.

## Facts

| Debt | Consequence today | Owner |
|---|---|---|
| `secret_ref` is TEXT, not a foreign key | the DB cannot refuse an orphaning delete, so a three-lock treaty (`vault.secrets` → `core.tenant_model_entries` → `core.tenant_model_selection`, all `FOR UPDATE`) is hand-held by every producer and destroyer of a credential reference | §V2-1 |
| the credential payload is the authority for `provider` (`meta_provider` is a copy, not a contract) | activation must decrypt an envelope to write a truthful row | §V2-3 |
| the selection row snapshots `provider` + `context_cap_tokens` at write time | the catalogue must be consulted at write (a TOCTOU the gate-and-write statement closes), and a later admin repoint or cap change is invisible to the stored view | §V2-2 |

## V2-1 · Make the reference a foreign key — deletes the treaty

`vault.secrets.id` is already a UUID primary key. Referencing rows store it:

```sql
ALTER TABLE core.tenant_model_entries   ADD COLUMN secret_id UUID REFERENCES vault.secrets(id);
ALTER TABLE core.tenant_model_selection ADD COLUMN secret_id UUID REFERENCES vault.secrets(id);
```

An INSERT of a referencing row then takes `FOR KEY SHARE` on the credential
row; a DELETE of a referenced credential fails IN THE DATABASE. The
producer/destroyer race that `afd_vault/src/sql.rs`'s lock trio hand-simulates
(as the retired Zig daemon's `secret_reference_txn` module did before it) is
settled by Postgres, for every future reference producer too. The Zig module's
own first sentence conceded the point: "it cannot be a foreign key: secret_ref
is TEXT" — a fact about the columns, not about possibility.

Migration shape: add column → backfill by join on `(workspace, key_name)` →
dual-write → flip readers → keep `secret_ref` TEXT for display only (or derive
it). Single-writer only: this lands after the Zig daemon is retired, never
during the soak.

**Deletes:** both implementations of the lock treaty, the deadlock-order
contract between crates, and the orphan class.

## V2-2 · Store intent, resolve facts live — deletes the snapshot class

The selection row stores four things; two are copies. Resolution already
ignores the stored `provider` (the credential supplies it) and platform mode
already ignores the whole row in favour of the live default. v2 stores only
what the tenant decided:

```
core.tenant_model_selection v2:  (tenant_id, mode, secret_id, model)
    context_cap_tokens → resolved at lease time from core.model_library
    provider           → resolved at lease time from the credential
```

The write-time catalogue check survives as a UX courtesy (`SELECT EXISTS`),
but nothing is persisted from the catalogue, so there is nothing to race over
and nothing to go stale. An admin raising a model's cap takes effect on the
tenant's next lease with no re-activation — the same live-read philosophy the
platform default already has. That also retires a quirk this port carried
across deliberately: an explicit platform row shows the SNAPSHOT taken when the
tenant reset, not the live default. It is not a cutover divergence — both
daemons behave identically, which is why the parity register has no row for it
— it is a product behaviour both implementations share, and V2-2 is where it
stops being one.

## V2-3 · Make the metadata a contract — deletes the decrypt

Enforce at seal time — the one write path — that `meta_provider`,
`meta_has_key` and `meta_base_url` are written iff they match the payload.
The metadata becomes authoritative, and activation never opens an envelope:
provider from `meta_provider`, dialability from `meta_has_key`, endpoint from
`meta_base_url`. The SSRF guard already re-runs at lease time, which is the
real security boundary; activation-time endpoint validation is a courtesy 400.
A refused AND an accepted activation both complete with no plaintext key ever
in memory. This also retires the legacy model-fallback read (a bare PUT
sourcing its model from the credential body), which M121 already declared
legacy — v2 requires the model in the body or on the registry entry.

## The endgame

```
PUT /v1/tenants/me/provider {mode:self_managed, secret_ref, model}
│ ① INSERT INTO core.tenant_model_entries (…, secret_id, …)
│     SELECT s.id, … FROM vault.secrets s JOIN <workspace bridge>
│     ON CONFLICT DO NOTHING            ── the FK is the lock and the rung-2 refusal
│ ② INSERT INTO core.tenant_model_selection (tenant_id, mode, secret_id, model)
│     ON CONFLICT (tenant_id) DO UPDATE ── plus the courtesy EXISTS gate
└ COMMIT
2 statements · no explicit locks · no treaty · no decrypt · nothing to go stale
```

From 15 round trips and two decrypts (Zig) → 7 and one (cutover parity floor)
→ 2 and none — but the last jump is bought with schema, not SQL.

## Sequencing

V2-1 first: it deletes shared machinery both later steps would otherwise have
to respect. Then V2-2, then V2-3. Each is independently shippable; all three
are single-writer migrations gated on LAND of the cutover (Zig daemon
retired). None of this runs during the soak — the parity milestone implements
the treaty as-is, and every such site carries a comment naming the section
here that deletes it.
