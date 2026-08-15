import { test } from "bun:test";
import assert from "node:assert/strict";
import os from "node:os";
import path from "node:path";

import { stateInternals } from "../src/lib/state.ts";
import { STATE_DIR_ENV } from "../src/lib/config-dir.ts";

// No process-environment save/restore dance here, deliberately: the suite
// preload (test/setup.ts) sets the process variable, so these passing proves
// resolution honours ONLY the supplied environment.

test("resolveStatePaths defaults to the XDG-style config directory from an empty supplied environment", () => {
  const paths = stateInternals.resolveStatePaths({});
  const expectedBase = path.join(os.homedir(), ".config", "agentsfleet");
  assert.equal(paths.baseDir, expectedBase);
  assert.equal(paths.credentialsPath, path.join(expectedBase, "credentials.json"));
  assert.equal(paths.workspacesPath, path.join(expectedBase, "workspaces.json"));
});

test("resolveStatePaths honors the supplied environment's state-dir override", () => {
  const paths = stateInternals.resolveStatePaths({ [STATE_DIR_ENV]: "/tmp/agentsfleet-state-test" });
  assert.equal(paths.baseDir, "/tmp/agentsfleet-state-test");
  assert.equal(paths.credentialsPath, "/tmp/agentsfleet-state-test/credentials.json");
  assert.equal(paths.workspacesPath, "/tmp/agentsfleet-state-test/workspaces.json");
});
