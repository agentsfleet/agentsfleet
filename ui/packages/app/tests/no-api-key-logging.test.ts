import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

// The raw `agt_t*` API key is revealed exactly once and is secret from the
// network boundary. These three files mint, return, and render that raw value;
// none may push it to a log sink. The type system can't catch a stray
// `console.log(result)`, so pin the secret-handling surface as strictly
// log-free. A legitimate future log here must first scope the secret away —
// that friction is the point.

const APP_ROOT = resolve(__dirname, "..");

const SECRET_HANDLING_FILES = [
  "app/(dashboard)/settings/api-keys/actions.ts",
  "app/(dashboard)/settings/api-keys/components/CreateApiKeyDialog.tsx",
  "lib/api/api_keys.ts",
];

// Strip block + line comments so prose mentioning `console` doesn't trip the gate.
function stripComments(src: string): string {
  return src.replace(/\/\*[\s\S]*?\*\//g, "").replace(/^\s*\/\/.*$/gm, "");
}

describe("raw API key never reaches a log sink", () => {
  it.each(SECRET_HANDLING_FILES)("%s contains no console.* call", (relPath) => {
    const code = stripComments(readFileSync(resolve(APP_ROOT, relPath), "utf8"));
    const hits = code.match(/\bconsole\s*\./g) ?? [];
    expect(hits, `${relPath}: ${hits.length} console.* call(s) — secret may leak to logs`).toEqual([]);
  });
});
