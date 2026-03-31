/**
 * Filesystem test helpers shared across spec-init and run-preview test suites.
 */
import { mkdirSync, rmSync } from "node:fs";
import { join } from "node:path";
import os from "node:os";

export function makeTmp() {
  const dir = join(os.tmpdir(), `zctl-test-${Date.now()}-${Math.random().toString(36).slice(2)}`);
  mkdirSync(dir, { recursive: true });
  return dir;
}

export function cleanup(dir) {
  try { rmSync(dir, { recursive: true, force: true }); } catch {}
}

/** Simple parseFlags for use in tests that bypasses the real arg parser. */
export function parseFlags(tokens) {
  const options = {};
  for (let i = 0; i < tokens.length; i++) {
    if (tokens[i].startsWith("--")) {
      const key = tokens[i].slice(2);
      const next = tokens[i + 1];
      if (next && !next.startsWith("--")) { options[key] = next; i++; }
      else options[key] = true;
    }
  }
  return { options, positionals: [] };
}

export const writeLine = (s, l = "") => s.write(`${l}\n`);
