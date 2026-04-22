// `zombiectl zombie up` success-path formatting.
//
// On a successful install/deploy, stdout must include the literal line
// `🎉 Woohoo! Your zombie is installed and ready to run.` followed by
// the webhook URL returned by the API. This is the operator's first
// confirmation that the zombie is live, so the text is part of the CLI
// contract.

import { test } from "bun:test";
import assert from "node:assert/strict";
import { commandZombie } from "../src/commands/zombie.js";
import { makeNoop, ui, WS_ID } from "./helpers.js";
import { parseFlags } from "../src/program/args.js";
import { tmpdir } from "node:os";
import { mkdtempSync, writeFileSync, mkdirSync } from "node:fs";
import { join } from "node:path";

function makeStdout() {
  const lines = [];
  return {
    write: (s) => lines.push(s),
    lines,
  };
}

function makeDeps(overrides = {}) {
  return {
    parseFlags,
    request: async () => ({
      zombie_id: "zom_01abc",
      webhook_url: "https://api.usezombie.com/v1/webhooks/zom_01abc",
    }),
    apiHeaders: () => ({}),
    ui,
    printJson: () => {},
    printKeyValue: () => {},
    printSection: () => {},
    writeLine: (stream, line = "") => stream.write(line + "\n"),
    writeError: () => {},
    ...overrides,
  };
}

function setupZombieDir() {
  const dir = mkdtempSync(join(tmpdir(), "zombie-up-test-"));
  const zombieDir = join(dir, "lead-collector");
  mkdirSync(zombieDir);
  writeFileSync(
    join(zombieDir, "SKILL.md"),
    "---\nname: lead-collector\n---\n# skill\n",
  );
  writeFileSync(
    join(zombieDir, "TRIGGER.md"),
    "---\nname: lead-collector\n---\n# trigger\n",
  );
  return { dir, zombieDir };
}

const workspaces = { current_workspace_id: WS_ID, items: [] };

test("zombie up: stdout contains the Woohoo literal + webhook URL on success", async () => {
  const { dir } = setupZombieDir();
  const origCwd = process.cwd();
  process.chdir(dir);
  const stdout = makeStdout();
  try {
    const code = await commandZombie(
      { stdout, stderr: makeNoop(), jsonMode: false, noInput: false },
      ["up"],
      workspaces,
      makeDeps(),
    );
    assert.equal(code, 0);
    const out = stdout.lines.join("");
    assert.ok(
      out.includes("🎉 Woohoo! Your zombie is installed and ready to run."),
      `stdout missing Woohoo line:\n${out}`,
    );
    assert.ok(
      out.includes("Webhook URL: https://api.usezombie.com/v1/webhooks/zom_01abc"),
      `stdout missing labelled webhook URL:\n${out}`,
    );
  } finally {
    process.chdir(origCwd);
  }
});

test("zombie up: JSON mode does not print the Woohoo line", async () => {
  const { dir } = setupZombieDir();
  const origCwd = process.cwd();
  process.chdir(dir);
  const stdout = makeStdout();
  let printedJson = null;
  try {
    await commandZombie(
      { stdout, stderr: makeNoop(), jsonMode: true, noInput: false },
      ["up"],
      workspaces,
      makeDeps({
        printJson: (_stream, payload) => {
          printedJson = payload;
        },
      }),
    );
    const out = stdout.lines.join("");
    assert.ok(!out.includes("Woohoo"), "JSON mode should not print prose");
    assert.ok(printedJson && printedJson.webhook_url);
  } finally {
    process.chdir(origCwd);
  }
});
