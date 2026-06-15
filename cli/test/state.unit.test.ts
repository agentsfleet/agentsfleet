import { test } from "bun:test";
import assert from "node:assert/strict";
import os from "node:os";
import path from "node:path";

import { stateInternals } from "../src/lib/state.ts";

test("resolveStatePaths defaults to XDG-style agentsfleet config directory", () => {
  const previous = process.env.AGENTSFLEET_STATE_DIR;
  delete process.env.AGENTSFLEET_STATE_DIR;
  try {
    const paths = stateInternals.resolveStatePaths();
    const expectedBase = path.join(os.homedir(), ".config", "agentsfleet");
    assert.equal(paths.baseDir, expectedBase);
    assert.equal(paths.credentialsPath, path.join(expectedBase, "credentials.json"));
    assert.equal(paths.workspacesPath, path.join(expectedBase, "workspaces.json"));
  } finally {
    if (previous !== undefined) process.env.AGENTSFLEET_STATE_DIR = previous;
  }
});

test("resolveStatePaths honors AGENTSFLEET_STATE_DIR override", () => {
  const previous = process.env.AGENTSFLEET_STATE_DIR;
  process.env.AGENTSFLEET_STATE_DIR = "/tmp/agentsfleet-state-test";
  try {
    const paths = stateInternals.resolveStatePaths();
    assert.equal(paths.baseDir, "/tmp/agentsfleet-state-test");
    assert.equal(paths.credentialsPath, "/tmp/agentsfleet-state-test/credentials.json");
    assert.equal(paths.workspacesPath, "/tmp/agentsfleet-state-test/workspaces.json");
  } finally {
    if (previous === undefined) delete process.env.AGENTSFLEET_STATE_DIR;
    else process.env.AGENTSFLEET_STATE_DIR = previous;
  }
});
