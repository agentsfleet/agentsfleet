// Unit tests for package.json — the npm manifest surface the binary-rename
// pass touched.
//
// Pins two facts with different lifetimes:
//   1. The bin mapping installs `agentsfleet` from ./dist/bin/agentsfleet.js
//      (flipped with the binary rename, permanent).
//   2. The package NAME is @agentsfleet/cli — the E9 cutover (first publish
//      under the @agentsfleet npm org, spec Dimension 4.5) landed via Indy's
//      offline publish. This assertion now guards against a regression back to
//      the retired @agentsfleet/agentctl name.

import { describe, test, expect } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const PKG_NAME = "@agentsfleet/cli";
const BIN_NAME = "agentsfleet";
const BIN_ENTRY = "./dist/bin/agentsfleet.js";

function readManifest(): Record<string, unknown> {
  const raw = readFileSync(join(import.meta.dir, "..", "package.json"), "utf8");
  return JSON.parse(raw) as Record<string, unknown>;
}

describe("test_cli_bin_name_agentsfleet", () => {
  test("bin key installs `agentsfleet` from the renamed dist entry", () => {
    const pkg = readManifest();
    expect(pkg.bin).toEqual({ [BIN_NAME]: BIN_ENTRY });
  });

  test("package name is @agentsfleet/cli (E9 cutover landed)", () => {
    const pkg = readManifest();
    expect(pkg.name).toBe(PKG_NAME);
  });
});
