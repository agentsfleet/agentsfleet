#!/usr/bin/env node
// Seed / refresh core.model_library from scripts/model-library-allowlist.json.
//
// One code path serves both cases on purpose: an empty catalogue emits INSERTs,
// a populated one emits UPSERTs for drift only. The monthly path is therefore
// exercised from day one rather than rotting as a rarely-run branch.
//
//   make seed-models             # fetch, diff against the live catalogue, report
//   make seed-models APPLY=1     # same, then apply
//
// Rates are billing data, so the default is review-then-write: the diff prints and
// nothing reaches the database until you pass APPLY=1. The SQL is piped straight to
// psql on apply — it is derived mechanically from the diff you just read, so there
// is nothing in it to review separately and no artifact worth keeping.
//
// Deliberately NOT a migration. Migrations are immutable history; rates are
// mutable operational data that changes monthly. core.model_library already
// exists (schema/003) — this needs no schema change.
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const ALLOWLIST = join(ROOT, "scripts", "model-library-allowlist.json");
const APPLY = process.argv.includes("--apply");

/**
 * How to reach Postgres. Defaults to a local `psql`, which takes the connection
 * URL as its first argument. Set SEED_PSQL to a full command that ALREADY targets
 * the right database and the URL is not appended — that is how the integration
 * lane reaches the compose container, where no host psql exists:
 *
 *   SEED_PSQL="docker compose exec -T postgres psql -U agentsfleet -d agentsfleetdb"
 */
const PSQL = (process.env.SEED_PSQL ?? "").trim();
const PSQL_ARGV = PSQL ? PSQL.split(/\s+/) : ["psql"];
const PSQL_TAKES_URL = !PSQL;
// Read api-source providers from committed fixtures instead of the network.
// Integration tests use this: seeding all 16 providers deterministically still
// exercises the field mapping, the per-token conversion, and the real SQL path,
// without going red because Pioneer had a bad afternoon.
const FIXTURES = process.argv.includes("--fixtures")
  ? join(ROOT, "samples", "fixtures", "model-library")
  : null;
// Regenerate the committed SQL fixture the Zig integration tests self-seed from.
// CI's zig container has neither node nor psql, so the tests cannot run this
// script — they exec the committed file statement-by-statement instead. The
// timestamp is pinned to the allowlist's verified_at so regeneration is
// byte-identical and the fixture never churns without a real rate change.
const EMIT_FIXTURE = process.argv.includes("--emit-fixture-sql");

const NANOS_PER_USD = 1_000_000_000;
const PER_TOKEN_TO_PER_MTOK = 1_000_000;
const MS_PER_DAY = 86_400_000;

/** USD per Mtok -> nanos per Mtok. Rounded: the column is BIGINT. */
const toNanos = (usdPerMtok) => Math.round(usdPerMtok * NANOS_PER_USD);
const usd = (nanos) => (nanos / NANOS_PER_USD).toFixed(6).replace(/0+$/, "").replace(/\.$/, ".0");
const sqlStr = (s) => `'${String(s).replace(/'/g, "''")}'`;
const dig = (obj, path) => path.split(".").reduce((o, k) => (o == null ? o : o[k]), obj);

function fail(msg) {
  console.error(`✗ ${msg}`);
  process.exit(1);
}

// ── Gather ────────────────────────────────────────────────────────────────────

/** Manual providers carry their rates inline, hand-verified against source_url. */
function fromManual(provider, cfg) {
  return cfg.models.map((m) => ({
    provider,
    model_id: m.model_id,
    context_cap_tokens: m.context_cap_tokens,
    input: toNanos(m.input),
    cached: toNanos(m.cached_input),
    output: toNanos(m.output),
    tier: m.tier ?? null,
    source_url: cfg.source_url,
  }));
}

/**
 * API providers publish machine-readable pricing; the allowlist only names which
 * ids to keep. An allowlisted id that has vanished upstream is reported rather
 * than silently dropped — that is usually a retirement worth acting on.
 */
async function fromApi(provider, cfg) {
  let body;
  if (FIXTURES) {
    body = JSON.parse(readFileSync(join(FIXTURES, `${provider}.json`), "utf8"));
  } else {
    const res = await fetch(cfg.endpoint, { headers: { accept: "application/json" } });
    if (!res.ok) fail(`${provider}: ${cfg.endpoint} returned ${res.status}`);
    body = await res.json();
  }
  // Fail on empty, not just non-array: `{ data: [] }`, `{}`, and `[]` are all
  // "valid" shapes that would seed ZERO rows for this provider while the run
  // reports success — and a tenant later cannot activate any of its models.
  // For an API provider we allowlisted models from, an empty catalogue is an
  // outage or a shape change, never a state to continue past.
  const list = Array.isArray(body) ? body : (body.data ?? body.models);
  if (!Array.isArray(list) || list.length === 0) {
    fail(`${provider}: could not find a non-empty model array in the response`);
  }

  const scale = cfg.rate_unit === "per_token" ? PER_TOKEN_TO_PER_MTOK : 1;
  const byId = new Map(list.map((row) => [dig(row, cfg.field_map.model_id), row]));
  const out = [];

  for (const id of cfg.models) {
    const row = byId.get(id);
    if (!row) {
      console.warn(`  ! ${provider}/${id} — allowlisted but absent upstream (retired?)`);
      continue;
    }
    const input = Number(dig(row, cfg.field_map.input));
    const cachedRaw = dig(row, cfg.field_map.cached_input);
    // No published cache-read price -> fall back to the input rate, never 0.
    // A 0 here would silently zero-rate every cached read.
    const cached = cachedRaw == null || cachedRaw === "" ? input : Number(cachedRaw);
    const output = Number(dig(row, cfg.field_map.output));
    const ctx = Number(dig(row, cfg.field_map.context_cap_tokens));
    if (![input, cached, output, ctx].every(Number.isFinite)) {
      console.warn(`  ! ${provider}/${id} — unparseable rate or context, skipped`);
      continue;
    }
    out.push({
      provider,
      model_id: id,
      context_cap_tokens: ctx,
      input: toNanos(input * scale),
      cached: toNanos(cached * scale),
      output: toNanos(output * scale),
      tier: null,
      source_url: cfg.endpoint,
    });
  }
  // Same failure one level deeper: a NON-empty response where zero allowlisted
  // ids resolved (provider renamed its whole line) must not seed "successfully"
  // with the provider absent. Individual misses stay warnings on purpose — one
  // retired model should not block the rest.
  if (out.length === 0) {
    fail(`${provider}: none of the ${cfg.models.length} allowlisted ids matched the response`);
  }
  return out;
}

async function collect(allowlist) {
  const rows = [];
  for (const [provider, cfg] of Object.entries(allowlist.providers)) {
    if (cfg.source === "manual") {
      rows.push(...fromManual(provider, cfg));
    } else if (cfg.source === "api") {
      console.log(FIXTURES ? `→ reading ${provider} fixture` : `→ fetching ${provider} (${cfg.endpoint})`);
      rows.push(...(await fromApi(provider, cfg)));
    } else {
      fail(`${provider}: unknown source "${cfg.source}" (expected "manual" or "api")`);
    }
  }
  return rows;
}

// ── Compare ───────────────────────────────────────────────────────────────────

/**
 * Reads the live catalogue so the diff is against reality, not a checked-in
 * snapshot. No DATABASE_URL (or no psql) is not an error — it just means every
 * row reads as new, which is exactly the fresh-install case.
 */
function readLive() {
  const url = process.env.DATABASE_URL;
  if (!url && PSQL_TAKES_URL) {
    console.log("→ DATABASE_URL unset — treating the catalogue as empty (fresh-install mode)");
    return new Map();
  }
  const sql =
    "SELECT provider, model_id, context_cap_tokens, input_nanos_per_mtok, " +
    "cached_input_nanos_per_mtok, output_nanos_per_mtok FROM core.model_library";
  let raw;
  try {
    const [bin, ...pre] = PSQL_ARGV;
    const args = PSQL_TAKES_URL ? [...pre, url] : pre;
    raw = execFileSync(bin, [...args, "-At", "-F", "\t", "-c", sql], { encoding: "utf8" });
  } catch (err) {
    fail(`could not read core.model_library (${PSQL_ARGV.join(" ")}): ${err.message}`);
  }
  const live = new Map();
  for (const line of raw.split("\n").filter(Boolean)) {
    const [provider, model_id, ctx, input, cached, output] = line.split("\t");
    live.set(`${provider} ${model_id}`, {
      context_cap_tokens: Number(ctx),
      input: Number(input),
      cached: Number(cached),
      output: Number(output),
    });
  }
  return live;
}

function diff(wanted, live) {
  const added = [];
  const changed = [];
  for (const row of wanted) {
    const cur = live.get(`${row.provider} ${row.model_id}`);
    if (!cur) {
      added.push(row);
      continue;
    }
    const deltas = ["input", "cached", "output", "context_cap_tokens"]
      .filter((f) => cur[f] !== row[f])
      .map((f) => ({ field: f, from: cur[f], to: row[f] }));
    if (deltas.length) changed.push({ row, deltas });
  }
  const seen = new Set(wanted.map((r) => `${r.provider} ${r.model_id}`));
  const orphaned = [...live.keys()].filter((k) => !seen.has(k)).map((k) => k.split(" "));
  return { added, changed, orphaned };
}

function report({ added, changed, orphaned }) {
  console.log(`\n── diff ──────────────────────────────────────────────`);
  for (const r of added) {
    console.log(`  + ${r.provider}/${r.model_id}  ${usd(r.input)} / ${usd(r.cached)} / ${usd(r.output)}`);
  }
  for (const { row, deltas } of changed) {
    const parts = deltas.map((d) =>
      d.field === "context_cap_tokens"
        ? `ctx ${d.from} → ${d.to}`
        : `${d.field} ${usd(d.from)} → ${usd(d.to)}`,
    );
    console.log(`  ~ ${row.provider}/${row.model_id}  ${parts.join(", ")}`);
  }
  for (const [provider, model_id] of orphaned) {
    // Never auto-deleted: an operator may have added it deliberately from the UI.
    console.log(`  ? ${provider}/${model_id} — in the catalogue but not the allowlist (left alone)`);
  }
  if (!added.length && !changed.length) console.log("  (no changes)");
  console.log(`\n  ${added.length} new · ${changed.length} changed · ${orphaned.length} unmanaged`);
}

// ── Emit ──────────────────────────────────────────────────────────────────────

/**
 * core.model_library.id is a plain PK carrying a UUIDv7 shape CHECK (the '7' at
 * text position 15), not a GENERATED column — so the seed must supply one.
 */
function idFor(provider, modelId) {
  // Content hash of (provider, model_id) ALONE — stable across allowlist
  // reordering and re-runs, so a refresh never shifts an existing row's id.
  // (A position-derived id collides the primary key — which ON CONFLICT
  // (provider, model_id) does not arbitrate — the moment a model is inserted
  // mid-list.) The NUL separator cannot appear in either part, so distinct
  // pairs cannot alias one digest. Nibbles are forced to '7'/'8' to satisfy
  // the ck_model_library_id_uuidv7 shape CHECK and the RFC 4122 variant.
  const h = createHash("sha256").update(`${provider} ${modelId}`).digest("hex");
  return `${h.slice(0, 8)}-${h.slice(8, 12)}-7${h.slice(12, 15)}-8${h.slice(15, 18)}-${h.slice(18, 30)}`;
}

function emit(rows, allowlist, stamp, opts = {}) {
  const lines = [
    `-- core.model_library seed / refresh`,
    `-- Generated by scripts/seed-models.mjs on ${stamp}`,
    `-- Allowlist verified_at: ${allowlist.verified_at}`,
    `--`,
    `-- Rates are nanos per 1M tokens (1 nano = 1e-9 USD). cached_* is the cache-READ`,
    `-- price. Rows marked "tier: upper" are context-tiered models seeded at the HIGHER`,
    `-- rate — see tiering_note in the allowlist.`,
    `--`,
    `-- ON CONFLICT DO UPDATE: refresh corrects drift. Operator-added rows absent from`,
    `-- the allowlist are never touched.`,
    ``,
  ];
  if (!opts.no_transaction) lines.push(`BEGIN;`, ``);
  rows.forEach((r) => {
    if (r.tier) lines.push(`-- ${r.provider}/${r.model_id}: upper tier seeded (see allowlist)`);
    lines.push(`-- source: ${r.source_url}`);
    lines.push(
      `INSERT INTO core.model_library (id, provider, model_id, context_cap_tokens, ` +
        `input_nanos_per_mtok, cached_input_nanos_per_mtok, output_nanos_per_mtok, ` +
        `created_at, updated_at) VALUES (`,
    );
    lines.push(
      `  '${idFor(r.provider, r.model_id)}', ${sqlStr(r.provider)}, ${sqlStr(r.model_id)}, ` +
        `${r.context_cap_tokens}, ${r.input}, ${r.cached}, ${r.output}, ${stamp.ms}, ${stamp.ms})`,
    );
    lines.push(`ON CONFLICT (provider, model_id) DO UPDATE SET`);
    lines.push(
      `  context_cap_tokens = EXCLUDED.context_cap_tokens, ` +
        `input_nanos_per_mtok = EXCLUDED.input_nanos_per_mtok,`,
    );
    lines.push(
      `  cached_input_nanos_per_mtok = EXCLUDED.cached_input_nanos_per_mtok, ` +
        `output_nanos_per_mtok = EXCLUDED.output_nanos_per_mtok,`,
    );
    lines.push(`  updated_at = EXCLUDED.updated_at;`);
    lines.push(``);
  });
  if (!opts.no_transaction) lines.push(`COMMIT;`);
  return lines.join("\n");
}

function warnStale(allowlist, nowMs) {
  const verified = Date.parse(allowlist.verified_at);
  if (!Number.isFinite(verified)) return;
  const ageDays = Math.floor((nowMs - verified) / MS_PER_DAY);
  const limit = allowlist.stale_after_days ?? 45;
  if (ageDays <= limit) return;
  console.warn(
    `\n  ! allowlist verified_at is ${ageDays} days old (limit ${limit}). Manual-source ` +
      `rates may have drifted — re-verify against each provider's source_url before applying.`,
  );
}

// ── Main ──────────────────────────────────────────────────────────────────────

const allowlist = JSON.parse(readFileSync(ALLOWLIST, "utf8"));
const nowMs = Date.now();
const stamp = Object.assign(new Date(nowMs).toISOString().slice(0, 10), { ms: nowMs });

const wanted = await collect(allowlist);
console.log(`→ ${wanted.length} allowlisted rows across ${Object.keys(allowlist.providers).length} providers`);

if (EMIT_FIXTURE) {
  const { writeFileSync } = await import("node:fs");
  const fixed_ms = Date.parse(allowlist.verified_at);
  const fixture_stamp = Object.assign(allowlist.verified_at, { ms: fixed_ms });
  const out = join(ROOT, "samples", "fixtures", "model-library", "seed.sql");
  // No BEGIN/COMMIT: the Zig tests exec one statement at a time.
  writeFileSync(out, emit(wanted, allowlist, fixture_stamp, { no_transaction: true }) + "\n");
  console.log(`→ wrote ${out} (${wanted.length} rows, stamp ${allowlist.verified_at})`);
  process.exit(0);
}

const delta = diff(wanted, readLive());
report(delta);
warnStale(allowlist, nowMs);

if (!delta.added.length && !delta.changed.length) {
  console.log("\n✓ catalogue already matches the allowlist — nothing to emit");
  process.exit(0);
}

if (!APPLY) {
  console.log("\n  Re-run with APPLY=1 to write these changes to the database.");
  process.exit(0);
}
if (PSQL_TAKES_URL && !process.env.DATABASE_URL) fail("APPLY=1 needs DATABASE_URL (or SEED_PSQL)");
{
  const [bin, ...pre] = PSQL_ARGV;
  const args = PSQL_TAKES_URL ? [...pre, process.env.DATABASE_URL] : pre;
  // Only the rows the diff named: upserting all 77 would bump updated_at_ms on
  // unchanged rows and destroy it as a per-row drift signal.
  const toWrite = [...delta.added, ...delta.changed.map((c) => c.row)];
  execFileSync(bin, [...args, "-v", "ON_ERROR_STOP=1", "-f", "-"], {
    input: emit(toWrite, allowlist, stamp),
    stdio: ["pipe", "inherit", "inherit"],
  });
}
console.log(`✓ applied — ${delta.added.length} added, ${delta.changed.length} updated`);
// The in-process rate cache is populated at boot (cmd/serve.zig) and after admin
// API mutations (model_library_admin.zig) — never by a direct SQL write. Until it
// is rebuilt, the activation gate still reads the OLD catalogue: named providers
// keep failing UZ-PROVIDER-004 and the provider dropdown stays empty, which reads
// exactly like the seed having done nothing.
console.log(
  "\n  ! Restart agentsfleetd before these take effect.\n" +
    "    The rate cache is built at boot and on admin-API writes; a direct SQL\n" +
    "    write does not rebuild it, so until you restart, activation still sees\n" +
    "    the old catalogue and fails with UZ-PROVIDER-004.",
);
