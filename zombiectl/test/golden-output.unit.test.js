// Golden text fixtures for byte-exact CLI output.
//
// BUN_RULES §8 forbids snapshot tests by default. Golden text fixtures
// are the carve-out (recorded in spec Discovery): the assertion *is*
// the byte-exact rendering, the fixture is checked into source as a
// reviewable .txt file, and updates are deliberate (regenerate via
// the makeGolden helper at the bottom, then commit).

import { describe, test, expect } from "bun:test";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { runCli, VERSION } from "../src/cli.js";
import { makeBufferStream } from "./helpers.js";

const TEST_DIR = dirname(fileURLToPath(import.meta.url));
const GOLDEN_DIR = join(TEST_DIR, "golden");

// Pin the version that lived in the fixture when it was captured. The
// fixture itself is rewritten in lockstep with VERSION via the regenerate
// recipe below; the substitution here lets `make sync-version` ship a
// VERSION bump without immediately breaking this test on the same diff.
const FIXTURE_VERSION = "0.34.0";

function golden(name) {
  const raw = readFileSync(join(GOLDEN_DIR, name), "utf8");
  // Substitute `zombiectl vX.Y.Z` with the current VERSION so a
  // version-only bump (no rendering changes) doesn't break the byte-exact
  // assertion. Anchored to the literal banner prefix so unrelated text
  // that happens to contain "v0.34.0" elsewhere is left alone. If
  // FIXTURE_VERSION drifts from what's actually in the fixture file, the
  // regeneration recipe at the bottom catches it.
  return raw.replaceAll(`zombiectl v${FIXTURE_VERSION}`, `zombiectl v${VERSION}`);
}

describe("golden — --version under NO_COLOR is byte-exact", () => {
  test("matches test/golden/version-no-color.txt", async () => {
    const out = makeBufferStream();
    const code = await runCli(["--version"], {
      stdout: out.stream,
      stderr: makeBufferStream().stream,
      env: { NO_COLOR: "1" },
    });
    expect(code).toBe(0);
    expect(out.read()).toBe(golden("version-no-color.txt"));
  });
});

describe("golden — --help under NO_COLOR is byte-exact", () => {
  test("matches test/golden/help-no-color.txt", async () => {
    const out = makeBufferStream();
    const code = await runCli(["--help"], {
      stdout: out.stream,
      stderr: makeBufferStream().stream,
      env: { NO_COLOR: "1" },
    });
    expect(code).toBe(0);
    expect(out.read()).toBe(golden("help-no-color.txt"));
  });

  test("every line ≤80 columns", () => {
    const overflow = golden("help-no-color.txt")
      .split("\n")
      .filter((line) => line.length > 80);
    expect(overflow).toEqual([]);
  });

  test("contains no ANSI escape sequences", () => {
    expect(golden("help-no-color.txt")).not.toMatch(/\x1b\[/);
  });

  test("contains no decorative emoji or box-drawing chars", () => {
    const txt = golden("help-no-color.txt");
    expect(txt).not.toContain("🧟");
    expect(txt).not.toContain("🎉");
    expect(txt).not.toMatch(/[╭╮╯╰│]/);
  });
});

// To regenerate: NO_COLOR=1 node zombiectl/bin/zombiectl.js --help \
//   > zombiectl/test/golden/help-no-color.txt && \
//   NO_COLOR=1 node zombiectl/bin/zombiectl.js --version \
//   > zombiectl/test/golden/version-no-color.txt
// Then bump FIXTURE_VERSION above to match the current VERSION,
// verify the diff is intentional, and commit.
