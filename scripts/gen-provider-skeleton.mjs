#!/usr/bin/env node
// Regenerate the provider skeleton of scripts/model-library-allowlist.json from
// the vendored NullClaw source.
//
//   node scripts/gen-provider-skeleton.mjs            # diff against the current file
//   node scripts/gen-provider-skeleton.mjs --write    # rewrite it
//
// WHY THIS EXISTS
//
// The allowlist used to hand-copy provider ids from NullClaw. A hand-copy of a
// vendored table is a drift generator: bumping the dependency silently changed
// what the platform could dial while the allowlist kept its old opinion, and
// `pioneer` sat in the file for months naming a provider NullClaw has never
// heard of. Provider identity, endpoint, and display label are now DERIVED, so
// a dependency bump either regenerates cleanly or fails this script.
//
// WHAT IS DERIVED vs WHAT IS CURATED
//
//   derived (never hand-edit)  id, aliases, dial, display
//   curated (hand-owned)       base_url, source, source_url, endpoint,
//                              field_map, rate_unit, models, unpriced_reason,
//                              rate_basis, notes
//
// base_url is curated even though NullClaw publishes one: for a provider we have
// never seen the derived value is the right default, but for one we have aimed
// somewhere deliberate (see nullclaw_alias_collisions) regenerating it is a
// silent wrong-endpoint bug. An existing base_url therefore always wins.
//
// A regeneration preserves every curated field by canonical id and only ever
// rewrites the derived ones, so running this never costs a rate.
//
// THE ONE INVARIANT
//
// The allowlist may not name a provider NullClaw cannot dial (Indy, Aug 15
// 2026). Two dial routes satisfy it:
//
//   dial: "native"    NullClaw resolves the name itself — core_providers or
//                     compat_providers. The runtime owns the endpoint.
//   dial: "endpoint"  NullClaw does NOT know the name. Reached only as an
//                     OpenAI-compatible custom endpoint, which means a
//                     base_url is mandatory and the caller must route through
//                     `custom:<base_url>` rather than the bare name.
//
// `pioneer` is the only "endpoint" provider today. Its rows stay priced because
// core.platform_provider_defaults carries a restricting foreign key onto
// (provider, model_id) and the platform default reaches it by base_url — but it
// is never offered as a credential provider id, because the bare name classifies
// as .unknown and cannot dial.
import { readFileSync, readdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const ALLOWLIST = join(ROOT, "scripts", "model-library-allowlist.json");
const WRITE = process.argv.includes("--write");

// Providers NullClaw implements by spawning a local coding-agent binary. They
// authenticate through that binary's own session and carry no API key, so a
// priced catalogue row for one could never be billed or dialled.
const CLI_ENGINE_KINDS = new Set([
  "claude_cli_provider",
  "codex_cli_provider",
  "gemini_cli_provider",
  "openai_codex_provider",
]);

/** Locate the vendored NullClaw package (the version is in the directory name). */
function nullclawSrc() {
  const pkgDir = join(ROOT, "zig-pkg");
  const hit = readdirSync(pkgDir).find((d) => d.startsWith("nullclaw-"));
  if (!hit) throw new Error("no vendored nullclaw- package under zig-pkg/ — run a build first");
  return { dir: join(pkgDir, hit, "src"), version: hit.split("-")[1] };
}

/** alias -> canonical, read from the `canonicalProviderName` arms. */
function parseAliases(src) {
  const body = readFileSync(join(src, "provider_names.zig"), "utf8");
  const fn = body.slice(body.indexOf("pub fn canonicalProviderName("), body.indexOf("pub fn canonicalProviderNameIgnoreCase("));
  const map = new Map();
  for (const line of fn.split("\n")) {
    const target = line.match(/return "([a-z0-9._-]+)";/);
    if (!target) continue;
    for (const m of line.matchAll(/name, "([a-z0-9._-]+)"\)/g)) map.set(m[1], target[1]);
  }
  return map;
}

/** compat_providers rows: OpenAI-compatible dial targets with an explicit URL. */
function parseCompat(src) {
  const body = readFileSync(join(src, "providers", "factory.zig"), "utf8");
  const table = body.slice(body.indexOf("const compat_providers = [_]CompatProvider{"), body.indexOf("\n};", body.indexOf("const compat_providers")));
  const out = new Map();
  for (const m of table.matchAll(/\.\{\s*\.name = "([^"]+)"(?:,\s*\.url = "([^"]+)")?(?:,\s*\.display = "([^"]+)")?/g)) {
    out.set(m[1], { url: m[2] ?? null, display: m[3] ?? null });
  }
  return out;
}

/** core_providers: natively implemented; the endpoint lives in the implementation. */
function parseCore(src) {
  const body = readFileSync(join(src, "providers", "factory.zig"), "utf8");
  const table = body.slice(body.indexOf("const core_providers = std.StaticStringMap"), body.indexOf("});", body.indexOf("const core_providers")));
  const out = new Map();
  for (const m of table.matchAll(/\.\{\s*"([a-z0-9._-]+)",\s*\.([a-z_]+)\s*\}/g)) out.set(m[1], m[2]);
  return out;
}

const { dir: SRC, version: NULLCLAW_VERSION } = nullclawSrc();
const aliases = parseAliases(SRC);
const compat = parseCompat(SRC);
const core = parseCore(SRC);
const canonical = (n) => aliases.get(n) ?? n;

// Every name NullClaw answers to, folded onto its canonical id.
const byCanonical = new Map();
const register = (name) => {
  const id = canonical(name);
  if (!byCanonical.has(id)) byCanonical.set(id, { id, aliases: new Set() });
  if (name !== id) byCanonical.get(id).aliases.add(name);
};
for (const n of compat.keys()) register(n);
for (const n of core.keys()) register(n);
for (const n of aliases.keys()) register(n);

// Drop the CLI engines: no key, so no credential and no priced row.
for (const [id] of byCanonical) {
  if (CLI_ENGINE_KINDS.has(core.get(id))) byCanonical.delete(id);
}

// Endpoint + label, preferring the canonical row and falling back to any alias
// that carries them (`together` has the URL where `together-ai` may not).
for (const entry of byCanonical.values()) {
  const rows = [entry.id, ...entry.aliases].map((n) => compat.get(n)).filter(Boolean);
  entry.base_url = rows.find((r) => r.url)?.url ?? null;
  entry.display = rows.find((r) => r.display)?.display ?? null;
  entry.native = true;
}

const current = JSON.parse(readFileSync(ALLOWLIST, "utf8"));
const CURATED = [
  "source", "source_url", "endpoint", "rate_unit", "field_map", "models",
  "unpriced_reason", "rate_basis", "note", "notes",
];

const providers = {};
const stats = { native: 0, endpoint: 0, priced: 0, unpriced: 0, newlyAdded: [] };

// Curated order first, newcomers alphabetically after it. The existing order is
// flagship-first, not alphabetical, and it is what makes the file scannable —
// re-sorting it would churn every rate line and reshuffle the emitted SQL on a
// change that touched no rate.
const priorOrder = Object.keys(current.providers).filter((id) => byCanonical.has(id));
const newcomers = [...byCanonical.keys()].filter((id) => !current.providers[id]).sort();

for (const id of [...priorOrder, ...newcomers]) {
  const derived = byCanonical.get(id);
  const prev = current.providers[id] ?? null;
  const row = { dial: "native" };
  if (derived.aliases.size) row.aliases = [...derived.aliases].sort();
  // base_url is CURATED, not derived: an existing one always wins. NullClaw's
  // table is the right default for a provider we have never seen, and the wrong
  // answer for one we have deliberately pointed elsewhere — `kimi` and `qwen`
  // are priced from their international price pages, while NullClaw resolves
  // both names to the mainland-China endpoint. Deriving this field overwrote
  // that intent silently, which is the wrong-continent failure the allowlist's
  // own header warns about.
  if (prev?.base_url) row.base_url = prev.base_url;
  else if (derived.base_url) row.base_url = derived.base_url;
  if (derived.display) row.display = derived.display;
  for (const k of CURATED) if (prev?.[k] !== undefined) row[k] = prev[k];
  if (!prev) {
    stats.newlyAdded.push(id);
    // `source: manual` with no models is the seeder's no-op shape: it takes the
    // manual branch and emits nothing. Omitting `source` would hard-fail
    // seed-models.mjs, which rejects any provider whose source it cannot name.
    row.source = "manual";
    row.models = [];
    // `awaiting_curation` is the only reason code that IS a queue: a provider
    // NullClaw just started dialling that nobody has priced yet. Every other
    // code in the vocabulary records a decision. The next curation pass either
    // prices this provider or replaces the code with the reason it cannot.
    row.unpriced_reason = "awaiting_curation";
    row.note = "Newly derived from a NullClaw bump; no rates curated yet. Self-managed only until then (zero rates never enter the cost path — schema/400_model_library.sql).";
  }
  providers[id] = row;
  stats.native++;
  if (row.models?.length) stats.priced++;
  else stats.unpriced++;
}

// Endpoint-dialled providers: kept from the current file, never derived, and
// each one must carry the base_url that is its only route to a dial.
for (const [id, prev] of Object.entries(current.providers)) {
  if (providers[id]) continue;
  if (!prev.base_url) throw new Error(`${id}: NullClaw cannot dial it and it has no base_url — remove it or give it one`);
  providers[id] = { dial: "endpoint", ...prev };
  stats.endpoint++;
  if (prev.models?.length) stats.priced++;
}

// Restore the file's original key order exactly — including endpoint-dialled
// providers such as `pioneer`, which the two passes above would otherwise strand
// at the end. Newcomers follow, alphabetically. Without this the emitted SQL
// reshuffles and the fixture diff hides real changes among moved lines.
const ordered = {};
for (const id of Object.keys(current.providers)) if (providers[id]) ordered[id] = providers[id];
for (const id of Object.keys(providers).sort()) if (!ordered[id]) ordered[id] = providers[id];
if (Object.keys(ordered).length !== Object.keys(providers).length) {
  throw new Error("reordering lost a provider — refusing to write");
}

const revised = {
  _readme: current._readme,
  _generator: [
    "Provider identity, aliases, dial and display are GENERATED by",
    "scripts/gen-provider-skeleton.mjs from the vendored NullClaw source. Do not",
    "hand-edit them — rerun the generator after a dependency bump.",
    "Curated by hand: base_url, source, source_url, endpoint, rate_unit,",
    "field_map, models, unpriced_reason, rate_basis, notes.",
    "base_url is curated: the generator supplies one for a provider it has never",
    "seen, but never overwrites an existing one (see nullclaw_alias_collisions).",
    "",
    "dial=native   NullClaw resolves the name; the runtime owns the endpoint.",
    "dial=endpoint NullClaw does NOT know the name. Reachable only as an",
    "              OpenAI-compatible custom endpoint, so base_url is mandatory",
    "              and it is never offered as a credential provider id.",
  ],
  _generated_from: `nullclaw-${NULLCLAW_VERSION}`,
  verified_at: current.verified_at,
  stale_after_days: current.stale_after_days,
  tiering_note: current.tiering_note,
  // The reason vocabulary is curated prose, not derived state — carried through
  // verbatim. Dropping it would strand every unpriced_reason in the file
  // pointing at a legend that no longer exists.
  unpriced_reasons: current.unpriced_reasons,
  nullclaw_alias_collisions: current.nullclaw_alias_collisions,
  providers: ordered,
  retired: current.retired,
};

// Emit with model objects and alias lists kept on ONE line. Plain
// JSON.stringify reflows every rate onto four lines, which turns the next
// monthly refresh into an unreadable diff — this file is reviewed by a human
// before billing rates are written, so its line shape is load-bearing.
const ONE_LINE_KEYS = new Set(["model_id", "name"]);
const isCompactObject = (v) =>
  v !== null && typeof v === "object" && !Array.isArray(v) &&
  Object.keys(v).some((k) => ONE_LINE_KEYS.has(k));
const isShortStringArray = (v) =>
  Array.isArray(v) && v.every((e) => typeof e === "string") &&
  JSON.stringify(v).length <= 72;

function emit(value, indent) {
  const pad = "  ".repeat(indent);
  const inner = "  ".repeat(indent + 1);
  if (isCompactObject(value)) {
    const fields = Object.entries(value).map(([k, v]) => `${JSON.stringify(k)}: ${JSON.stringify(v)}`);
    return `{ ${fields.join(", ")} }`;
  }
  if (isShortStringArray(value)) return `[${value.map((e) => JSON.stringify(e)).join(", ")}]`;
  if (Array.isArray(value)) {
    if (value.length === 0) return "[]";
    return `[\n${value.map((e) => inner + emit(e, indent + 1)).join(",\n")}\n${pad}]`;
  }
  if (value !== null && typeof value === "object") {
    const keys = Object.keys(value);
    if (keys.length === 0) return "{}";
    return `{\n${keys.map((k) => `${inner}${JSON.stringify(k)}: ${emit(value[k], indent + 1)}`).join(",\n")}\n${pad}}`;
  }
  return JSON.stringify(value);
}

const json = `${emit(revised, 0)}\n`;
if (JSON.stringify(JSON.parse(json)) !== JSON.stringify(revised)) {
  throw new Error("emitter changed the data — refusing to write");
}
if (WRITE) {
  writeFileSync(ALLOWLIST, json);
  console.log(`wrote ${ALLOWLIST}`);
} else {
  writeFileSync(`${ALLOWLIST}.revised`, json);
  console.log(`wrote ${ALLOWLIST}.revised (diff it, then rerun with --write)`);
}

console.log(`\nnullclaw:        ${NULLCLAW_VERSION}`);
console.log(`providers:       ${stats.native + stats.endpoint}  (${stats.native} native · ${stats.endpoint} endpoint)`);
console.log(`priced:          ${stats.priced}`);
console.log(`awaiting rates:  ${stats.unpriced}`);
console.log(`newly added:     ${stats.newlyAdded.length}`);
