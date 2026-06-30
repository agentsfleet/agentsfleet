#!/usr/bin/env node
// Point every integration fixture at the shared `test_scope_tokens.zig` module
// for its JWKS / issuer / audience, so those live in ONE place. Keeps each
// file's const NAMES (usages unchanged) — only the value becomes a module ref.
//
//   node scripts/wire-shared-jwks.mjs           # dry-run
//   node scripts/wire-shared-jwks.mjs --apply
import { readFileSync, writeFileSync, globSync } from "node:fs";
import path from "node:path";

const APPLY = process.argv.includes("--apply");
const MODULE = "src/agentsfleetd/http/test_scope_tokens.zig";
const SHARED_N = JSON.parse(readFileSync(MODULE, "utf8").match(/"n":"([^"]+)"/)[1] ? `"${readFileSync(MODULE, "utf8").match(/"n":"([^"]+)"/)[1]}"` : '""');

const FILES = globSync("src/agentsfleetd/**/*.zig").filter((f) =>
  (/integration_test\.zig$|sse_test_fixtures\.zig$|idor_test\.zig$/.test(f)) && f !== MODULE);

let changed = 0;
for (const f of FILES) {
  let txt = readFileSync(f, "utf8"); const orig = txt;
  if (!txt.includes(SHARED_N)) continue; // only files carrying the shared JWKS

  // Relative import path from this file's dir to the module.
  let rel = path.relative(path.dirname(f), MODULE).replaceAll(path.sep, "/");
  if (!rel.startsWith(".")) rel = "./" + rel;

  // 1) inline JWKS const  ->  module ref
  txt = txt.replace(/const (\w+) =\n\s*\\\\\{"keys":[\s\S]*?\}\n;/g, (m, name) => `const ${name} = scope_fixtures.JWKS;`);
  // 2) issuer / audience consts -> module ref
  txt = txt.replace(/const (\w+) = "https:\/\/clerk\.test\.agentsfleet\.net";/g, (m, name) => `const ${name} = scope_fixtures.ISSUER;`);
  txt = txt.replace(/const (\w+) = "https:\/\/api\.agentsfleet\.net";/g, (m, name) => `const ${name} = scope_fixtures.AUDIENCE;`);

  // 3) add the import once, after the first @import line.
  if (!txt.includes("test_scope_tokens.zig")) {
    txt = txt.replace(/(const \w+ = @import\("[^"]+"\);\n)/, `$1const scope_fixtures = @import("${rel}");\n`);
  }
  if (txt !== orig) { changed++; console.log(`  ${APPLY ? "wired" : "would wire"} ${f.split("agentsfleetd/")[1]}  (import ${rel})`); if (APPLY) writeFileSync(f, txt); }
}
console.log(`\n${APPLY ? "Wired" : "Would wire"} ${changed} files.${APPLY ? "" : "  (dry-run)"}`);
