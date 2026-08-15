#!/usr/bin/env bun
// Regenerates cli/test/fixtures/nullclaw-providers.json from the vendored
// NullClaw source. Run after bumping the `.nullclaw` dependency in
// build.zig.zon — the parity test fails until the fixture records the new
// package, and this is what writes it:
//
//     bun run cli/scripts/gen-provider-fixture.ts
//
// Requires zig-pkg/ to be present (any `zig build` populates it). Extraction
// is anchored per block, matching providers-parity.unit.test.ts exactly: the
// factory also carries a `provider_holder_cases` test-vector array in the same
// `.name = "..."` shape, so a whole-file sweep would read fixtures as API.
//
// Regenerating widens or narrows a PUBLIC flag. Read the resulting diff: if
// PROVIDER_IDS moves, that is a user-visible change and belongs in a changelog.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

// One capture shape, three anchors — the quoted-value group is identical in
// all three Zig constructs, so it is spelled once (RULE UFS).
const QUOTED = String.raw`"([^"]+)"`;
const UTF8 = "utf8" as const;
const PARENT = ".." as const;

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), PARENT, PARENT);
const ZIG_PKG = path.join(REPO_ROOT, "zig-pkg");
const OUT = path.join(REPO_ROOT, "cli", "test", "fixtures", "nullclaw-providers.json");

const NAME_FIELD = new RegExp(String.raw`\.name = ${QUOTED}`, "g");
const MAP_KEY = new RegExp(String.raw`\.\{ ${QUOTED}`, "g");
const ALIAS_ARM = new RegExp(String.raw`name, ${QUOTED}\)`, "g");

function blockAfter(source: string, anchor: string, terminator: string): string {
  const start = source.indexOf(anchor);
  if (start < 0) throw new Error(`anchor not found: ${anchor}`);
  const end = source.indexOf(`\n${terminator}`, start);
  if (end <= start) throw new Error(`terminator not found for: ${anchor}`);
  return source.slice(start, end);
}

function matchAll(block: string, pattern: RegExp): string[] {
  return [...block.matchAll(pattern)].flatMap((m) => (m[1] === undefined ? [] : [m[1]]));
}

// The package build.zig.zon pins, so a stale sibling in the Zig cache cannot win.
const zon = fs.readFileSync(path.join(REPO_ROOT, "build.zig.zon"), UTF8);
const blockStart = zon.indexOf(".nullclaw = .{");
if (blockStart < 0) throw new Error("build.zig.zon has no .nullclaw dependency block");
const pinned = new RegExp(String.raw`\.hash = ${QUOTED}`).exec(zon.slice(blockStart, zon.indexOf("},", blockStart)));
if (pinned?.[1] === undefined) throw new Error("no .hash in the .nullclaw block");
const pkg = pinned[1];

if (!fs.existsSync(path.join(ZIG_PKG, pkg))) {
  throw new Error(`zig-pkg/${pkg} is absent — run a zig build first`);
}
const factory = fs.readFileSync(path.join(ZIG_PKG, pkg, "src/providers/factory.zig"), UTF8);
const names = fs.readFileSync(path.join(ZIG_PKG, pkg, "src/provider_names.zig"), UTF8);

const out = {
  pkg,
  compat: matchAll(
    blockAfter(factory, "const compat_providers = [_]CompatProvider{", "};"),
    NAME_FIELD,
  ).sort(),
  core: matchAll(
    blockAfter(factory, "const core_providers = std.StaticStringMap", "});"),
    MAP_KEY,
  ).sort(),
  aliases: [
    ...new Set(matchAll(blockAfter(names, "pub fn canonicalProviderName(", "}"), ALIAS_ARM)),
  ].sort(),
};

fs.writeFileSync(OUT, `${JSON.stringify(out, null, 2)}\n`);
process.stdout.write(
  `wrote ${path.relative(REPO_ROOT, OUT)} — pkg=${pkg} compat=${out.compat.length} core=${out.core.length} aliases=${out.aliases.length}\n`,
);
