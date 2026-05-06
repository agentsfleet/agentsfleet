import { test } from "bun:test";
import assert from "node:assert/strict";
import os from "node:os";
import path from "node:path";

import { stateInternals } from "../src/lib/state.js";

test("resolveStatePaths defaults to XDG-style zombiectl config directory", () => {
  const previous = process.env.ZOMBIE_STATE_DIR;
  delete process.env.ZOMBIE_STATE_DIR;
  try {
    const paths = stateInternals.resolveStatePaths();
    const expectedBase = path.join(os.homedir(), ".config", "zombiectl");
    assert.equal(paths.baseDir, expectedBase);
    assert.equal(paths.credentialsPath, path.join(expectedBase, "credentials.json"));
    assert.equal(paths.workspacesPath, path.join(expectedBase, "workspaces.json"));
  } finally {
    if (previous !== undefined) process.env.ZOMBIE_STATE_DIR = previous;
  }
});

test("resolveStatePaths honors ZOMBIE_STATE_DIR override", () => {
  const previous = process.env.ZOMBIE_STATE_DIR;
  process.env.ZOMBIE_STATE_DIR = "/tmp/zombiectl-state-test";
  try {
    const paths = stateInternals.resolveStatePaths();
    assert.equal(paths.baseDir, "/tmp/zombiectl-state-test");
    assert.equal(paths.credentialsPath, "/tmp/zombiectl-state-test/credentials.json");
    assert.equal(paths.workspacesPath, "/tmp/zombiectl-state-test/workspaces.json");
  } finally {
    if (previous === undefined) delete process.env.ZOMBIE_STATE_DIR;
    else process.env.ZOMBIE_STATE_DIR = previous;
  }
});
