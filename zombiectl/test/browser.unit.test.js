import { test } from "bun:test";
import assert from "node:assert/strict";

import { resolveBrowserCommand } from "../src/lib/browser.js";

test("resolveBrowserCommand disables browser launch when BROWSER=false on darwin", async () => {
  const resolved = await resolveBrowserCommand({ BROWSER: "false" }, "darwin");
  assert.equal(resolved.argv, null);
  assert.equal(resolved.reason, "browser-disabled");
});

test("resolveBrowserCommand disables browser launch when BROWSER=0 on linux", async () => {
  const resolved = await resolveBrowserCommand({ BROWSER: "0", DISPLAY: ":0" }, "linux");
  assert.equal(resolved.argv, null);
  assert.equal(resolved.reason, "browser-disabled");
});
