// The catalogue is a MIRROR, and this is what keeps it one. `--provider`
// promises to reject only names the runner cannot dial, so its accepted set is
// derived from the vendored NullClaw runtime rather than restated by hand —
// authoring it from the architecture doc's illustrative table is how it
// silently narrowed to 13 ids and started rejecting deepseek/cerebras/mistral.
//
// What "dialable" means is decided by ONE function, `classifyProvider`, which
// consults three blocks. This test extracts each block SEPARATELY and by
// anchor, never by sweeping the whole file: factory.zig also contains a
// `provider_holder_cases` array of test vectors in the same `.name = "..."`
// shape, so an unanchored regex reads fixtures as public API and the next
// upstream negative-path fixture (`.name = "unknown-provider"`) would land in
// a user-facing flag.

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

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");
const ZIG_PKG = path.join(REPO_ROOT, "zig-pkg");
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

async function readVendored(relative: string): Promise<string | null> {
  const entries = await fs.readdir(ZIG_PKG).catch(() => [] as string[]);
  const pkg = entries.find((e) => e.startsWith("nullclaw-"));
  if (pkg === undefined) return null;
  return fs.readFile(path.join(ZIG_PKG, pkg, relative), "utf8").catch(() => null);
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

async function dialableNames(): Promise<{
  compat: string[];
  core: string[];
  aliases: string[];
  union: Set<string>;
}> {
  const factory = await readVendored(FACTORY_RELATIVE);
  const names = await readVendored(NAMES_RELATIVE);
  // The vendored package is part of the checked-in dependency set; if it ever
  // moves, that is itself the drift this test exists to catch.
  expect(factory).not.toBeNull();
  expect(names).not.toBeNull();

  const compat = matchAll(
    blockAfter(factory as string, "const compat_providers = [_]CompatProvider{", "};"),
    NAME_FIELD,
  );
  const core = matchAll(
    blockAfter(factory as string, "const core_providers = std.StaticStringMap", "});"),
    MAP_KEY,
  );
  const aliases = matchAll(
    blockAfter(names as string, "pub fn canonicalProviderName(", "}"),
    ALIAS_ARM,
  );
  return { compat, core, aliases, union: new Set([...compat, ...core, ...aliases]) };
}

describe("PROVIDER_IDS mirrors what the vendored NullClaw runtime can dial", () => {
  test("each source block is extracted by anchor, at its pinned size", async () => {
    const { compat, core, aliases } = await dialableNames();
    expect(compat.length).toBe(COMPAT_DIAL_COUNT);
    expect(core.length).toBe(CORE_PROVIDER_COUNT);
    expect(aliases.length).toBe(ALIAS_COUNT);
  });

  test("the test-vector array is NOT read as public API", async () => {
    const factory = await readVendored(FACTORY_RELATIVE);
    // The fixture array exists and carries names in the dial-table's shape —
    // the precondition that makes an unanchored sweep wrong. If upstream ever
    // drops it this assertion fails loudly rather than quietly weakening.
    expect(factory).toContain("const provider_holder_cases");
    const fixtures = matchAll(
      blockAfter(factory as string, "const provider_holder_cases", "};"),
      NAME_FIELD,
    );
    expect(fixtures.length).toBeGreaterThan(0);

    const { compat } = await dialableNames();
    // Anchored extraction stops at the dial table, so no fixture-only name can
    // reach it. (Every fixture name happens to be a real provider today; that
    // is upstream's choice, not a guarantee, which is the whole point.)
    const block = blockAfter(
      factory as string,
      "const compat_providers = [_]CompatProvider{",
      "};",
    );
    expect(block).not.toContain("provider_holder_cases");
    expect(compat.length).toBeLessThan(matchAll(factory as string, NAME_FIELD).length);
  });

  test("the catalogue is exactly the dialable union, less the CLI engines, plus the sentinel", async () => {
    const { union } = await dialableNames();
    const expected = [...union]
      .filter((n) => !(CLI_ENGINE_PROVIDERS as readonly string[]).includes(n))
      .sort();
    const accepted = (PROVIDER_IDS as readonly string[])
      .filter((n) => n !== OPENAI_COMPATIBLE_PROVIDER)
      .sort();
    expect(accepted).toEqual(expected);
    expect(PROVIDER_IDS).toContain(OPENAI_COMPATIBLE_PROVIDER);
  });

  test("every rejected CLI engine is a name NullClaw really dials", async () => {
    // The carve-out is deliberate, not a typo list. If upstream drops one of
    // these the refusal message becomes a lie, so it fails here.
    const { union } = await dialableNames();
    for (const name of CLI_ENGINE_PROVIDERS) {
      expect(union.has(name)).toBe(true);
      expect(PROVIDER_IDS as readonly string[]).not.toContain(name);
    }
  });

  test("the custom-endpoint sentinel is carried and is not a NullClaw name", async () => {
    const { union } = await dialableNames();
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
