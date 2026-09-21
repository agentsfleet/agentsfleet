// `constants/env.ts` is the one place an environment-variable name is spelled.
// This suite fails the build on a second spelling anywhere under `src/`, so a
// reintroduced literal — or a dotted read of a name the module declares — is
// caught here rather than by a reviewer running the grep.

import { describe, expect, test } from "bun:test";
import path from "node:path";

import * as ENV from "../src/constants/env.ts";

// Every name the module exports, so a new env var is guarded the day it lands
// rather than the day someone remembers to add it here.
const ENV_NAMES = Object.values(ENV);
const DECLARATION_SITE = path.join("constants", "env.ts");

// Prose mentions the variables on purpose — operator help text ("set
// AGENTSFLEET_API_URL") and comments that use markdown backticks around a
// name. Only code may not spell one, so comment lines are dropped before the
// scan rather than narrowing which spellings count as a declaration.
const codeOnly = (source: string): string =>
  source
    .split("\n")
    .filter((line) => {
      const trimmedLine = line.trimStart();
      return !(
        trimmedLine.startsWith("//") ||
        trimmedLine.startsWith("*") ||
        trimmedLine.startsWith("/*")
      );
    })
    .join("\n");

// A name is spelled when it appears quoted as a value, or read as a dotted
// property (`env.AGENTSFLEET_DASHBOARD_URL`) — the second is the spelling a
// quoted-only match let through.
const spellingsOf = (name: string): ReadonlyArray<string> => [
  `"${name}"`,
  `'${name}'`,
  `\`${name}\``,
  `.${name}`,
];

describe("constants/env.ts", () => {
  test("no file under src/ spells an env variable except the declaration site", async () => {
    const srcRoot = new URL("../src/", import.meta.url).pathname;
    const glob = new Bun.Glob("**/*.ts");
    const candidates: string[] = [];
    for await (const rel of glob.scan(srcRoot)) {
      if (rel !== DECLARATION_SITE) candidates.push(rel);
    }
    const bodies = await Promise.all(
      candidates.map((rel) => Bun.file(path.join(srcRoot, rel)).text()),
    );
    const code = bodies.map((body) => codeOnly(body));
    for (const name of ENV_NAMES) {
      const offenders = candidates.filter((_, i) =>
        spellingsOf(name).some((spelling) => code[i]?.includes(spelling)),
      );
      expect(offenders).toEqual([]);
    }
  });
});
