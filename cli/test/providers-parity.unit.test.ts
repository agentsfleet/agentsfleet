// The catalogue is a MIRROR, and this is what keeps it one. `--provider`
// promises to reject only names the runner cannot dial, so its accepted set is
// derived from the vendored NullClaw runtime rather than restated by hand —
// authoring it from the architecture doc's illustrative table is how it
// silently narrowed to 13 ids and started rejecting deepseek/cerebras/mistral.
//
// What "dialable" means is decided by ONE function, `classifyProvider`, which
// consults three blocks. They are extracted SEPARATELY and by anchor, never by
// sweeping the file: factory.zig also carries a `provider_holder_cases` array
// of test vectors in the same `.name = "..."` shape, so an unanchored regex
// reads fixtures as public API and the next upstream negative-path fixture
// (`.name = "unknown-provider"`) would land in a user-facing flag.
//
// WHERE the extraction reads from is the subtle part. `zig-pkg/` is a Zig
// build artifact — gitignored, absent from a fresh
// clone and from the Bun-only CI lane that runs this file. Reading it directly
// made these tests pass on a machine that had run a Zig build and fail
// everywhere else. So the extraction is checked in as a fixture, and the drift
// guarantee is reconstructed from two halves:
//
//   1. Always, everywhere: the catalogue equals the fixture, AND the fixture
//      records the same NullClaw package `build.zig.zon` pins. `build.zig.zon`
//      IS committed, so a dependency bump changes it, fails the pin assertion,
//      and forces the fixture to be regenerated — which then moves the
//      catalogue. The flag cannot silently narrow or widen.
//   2. Where the vendored source is present (any developer machine that has
//      built, any Zig-capable lane): the fixture still matches the live source,
//      block for block. Skipped, loudly, where the source is absent — half (1)
//      is what makes that skip safe.
//
// Regenerate the fixture after a dependency bump:
//     bun run cli/scripts/gen-provider-fixture.ts

import { describe, test, expect } from "bun:test";
import fs from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import {
  CLI_ENGINE_PROVIDERS,
  PROVIDER_EXAMPLES,
  PROVIDER_IDS,
} from "../src/constants/providers.ts";
import { OPENAI_COMPATIBLE_PROVIDER } from "../src/constants/custom-endpoint.ts";
import fixture from "./fixtures/nullclaw-providers.json" with { type: "json" };

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");
const ZIG_PKG = path.join(REPO_ROOT, "zig-pkg");
const BUILD_ZON = path.join(REPO_ROOT, "build.zig.zon");
const FACTORY_RELATIVE = path.join("src", "providers", "factory.zig");
const NAMES_RELATIVE = path.join("src", "provider_names.zig");

// Counts are pinned, not merely compared, so a bump that ADDS providers is a
// visible diff here rather than a silent widening of a public flag.
const COMPAT_DIAL_COUNT = 98;
const CORE_PROVIDER_COUNT = 17;
const ALIAS_COUNT = 14;

const NAME_FIELD = /\.name = "([^"]+)"/g;
const MAP_KEY = /\.\{ "([^"]+)"/g;
const ALIAS_ARM = /name, "([^"]+)"\)/g;

const isCliEngine = (name: string): boolean =>
  (CLI_ENGINE_PROVIDERS as readonly string[]).includes(name);

/** The catalogue the fixture implies: the dialable union, less the CLI engines. */
function expectedFromFixture(): string[] {
  const union = new Set([...fixture.compat, ...fixture.core, ...fixture.aliases]);
  return [...union].filter((n) => !isCliEngine(n)).sort();
}

// Selected by the PINNED name, not by readdir order. The Zig cache provably
// holds multiple versions of one package at once (two `websocket-0.1.0-*`
// directories sit there today), so during a bump both the old and the new
// nullclaw would coexist and `startsWith` could validate against the stale one.
async function readVendored(relative: string): Promise<string | null> {
  const entries = await fs.readdir(ZIG_PKG).catch(() => [] as string[]);
  if (!entries.includes(fixture.pkg)) return null;
  return fs.readFile(path.join(ZIG_PKG, fixture.pkg, relative), "utf8").catch(() => null);
}

// Slice from an opening anchor to the first line that closes it at column 0.
// Zig indents table members, so an unindented terminator is unambiguous.
function blockAfter(source: string, anchor: string, terminator: string): string {
  const start = source.indexOf(anchor);
  expect(start).toBeGreaterThanOrEqual(0);
  const end = source.indexOf(`\n${terminator}`, start);
  expect(end).toBeGreaterThan(start);
  return source.slice(start, end);
}

function matchAll(block: string, pattern: RegExp): string[] {
  return [...block.matchAll(pattern)].flatMap((m) => (m[1] === undefined ? [] : [m[1]]));
}

describe("PROVIDER_IDS mirrors what the vendored NullClaw runtime can dial", () => {
  test("the fixture records the NullClaw package build.zig.zon pins", async () => {
    // The whole hermetic half rests on this: bump the dependency without
    // regenerating the fixture and this fails, everywhere, with no Zig
    // toolchain required.
    const zon = await fs.readFile(BUILD_ZON, "utf8");
    // Scoped to the `.nullclaw` block — build.zig.zon carries ~10 `.hash =`
    // entries, so a bare substring match would be satisfied by any dependency.
    const start = zon.indexOf(".nullclaw = .{");
    expect(start).toBeGreaterThanOrEqual(0);
    const block = zon.slice(start, zon.indexOf("},", start));
    expect(block).toContain(`.hash = "${fixture.pkg}"`);
  });

  test("each recorded block is at its pinned size", () => {
    expect(fixture.compat.length).toBe(COMPAT_DIAL_COUNT);
    expect(fixture.core.length).toBe(CORE_PROVIDER_COUNT);
    expect(fixture.aliases.length).toBe(ALIAS_COUNT);
  });

  test("the catalogue is exactly the dialable union, less the CLI engines, plus the sentinel", () => {
    const accepted = (PROVIDER_IDS as readonly string[])
      .filter((n) => n !== OPENAI_COMPATIBLE_PROVIDER)
      .sort();
    expect(accepted).toEqual(expectedFromFixture());
    expect(PROVIDER_IDS).toContain(OPENAI_COMPATIBLE_PROVIDER);
  });

  test("every rejected CLI engine is a name NullClaw really dials", () => {
    // The carve-out is deliberate, not a typo list. If upstream drops one of
    // these the refusal message becomes a lie, so it fails here.
    const union = new Set([...fixture.compat, ...fixture.core, ...fixture.aliases]);
    for (const name of CLI_ENGINE_PROVIDERS) {
      expect(union.has(name)).toBe(true);
      expect(PROVIDER_IDS as readonly string[]).not.toContain(name);
    }
  });

  test("the custom-endpoint sentinel is carried and is not a NullClaw name", () => {
    const union = new Set([...fixture.compat, ...fixture.core, ...fixture.aliases]);
    expect(PROVIDER_IDS).toContain(OPENAI_COMPATIBLE_PROVIDER);
    expect(union.has(OPENAI_COMPATIBLE_PROVIDER)).toBe(false);
  });

  test("every help-text example is a real member", () => {
    for (const example of PROVIDER_EXAMPLES) {
      expect(PROVIDER_IDS as readonly string[]).toContain(example);
    }
  });

  test("every id is unique — a duplicate would render twice in the rejection message", () => {
    expect(new Set(PROVIDER_IDS).size).toBe(PROVIDER_IDS.length);
  });
});

describe("the fixture still matches the vendored source", () => {
  // Runs wherever `zig-pkg/` exists. Where it does not — a fresh clone, the
  // Bun-only CI lane — these skip, and the build.zig.zon pin above is what
  // keeps the skip from hiding a bump.
  test("the three blocks extract, by anchor, to exactly what the fixture records", async () => {
    const factory = await readVendored(FACTORY_RELATIVE);
    const names = await readVendored(NAMES_RELATIVE);
    if (factory === null || names === null) {
      // Not a silent pass: the pin test above already failed if the dependency
      // moved, so absence here costs no coverage of the drift guarantee.
      console.warn("zig-pkg/ absent — fixture-vs-source drift check skipped");
      return;
    }

    const compat = matchAll(
      blockAfter(factory, "const compat_providers = [_]CompatProvider{", "};"),
      NAME_FIELD,
    );
    const core = matchAll(
      blockAfter(factory, "const core_providers = std.StaticStringMap", "});"),
      MAP_KEY,
    );
    const aliases = matchAll(blockAfter(names, "pub fn canonicalProviderName(", "}"), ALIAS_ARM);

    expect(compat.sort()).toEqual([...fixture.compat].sort());
    expect(core.sort()).toEqual([...fixture.core].sort());
    expect([...new Set(aliases)].sort()).toEqual([...fixture.aliases].sort());
  });

  test("the test-vector array is NOT read as public API", async () => {
    const factory = await readVendored(FACTORY_RELATIVE);
    if (factory === null) {
      console.warn("zig-pkg/ absent — fixture-isolation check skipped");
      return;
    }
    // The fixture array exists and carries names in the dial-table's shape —
    // the precondition that makes an unanchored sweep wrong. If upstream ever
    // drops it this assertion fails loudly rather than quietly weakening.
    expect(factory).toContain("const provider_holder_cases");
    const block = blockAfter(factory, "const compat_providers = [_]CompatProvider{", "};");
    expect(block).not.toContain("provider_holder_cases");
    // Anchored extraction stops at the dial table, so no fixture-only name can
    // reach it. (Every fixture name happens to be a real provider today; that
    // is upstream's choice, not a guarantee, which is the whole point.)
    expect(matchAll(block, NAME_FIELD).length).toBeLessThan(
      matchAll(factory, NAME_FIELD).length,
    );
  });
});
