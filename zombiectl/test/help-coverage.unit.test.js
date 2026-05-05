// Regression test: every command wired in the registry must appear in
// `zombiectl --help` output. Closes a drift class where adding a command to
// the dispatcher (so it works) without updating printHelp leaves users unable
// to discover it. The HELP text is the canonical user-facing surface; the
// registry is the canonical wired-in surface; they must agree.

import { test } from "bun:test";
import assert from "node:assert/strict";
import { Writable } from "node:stream";

import { printHelp } from "../src/program/io.js";
import { registerProgramCommands } from "../src/program/command-registry.js";

function captureHelp() {
  let output = "";
  const stdout = new Writable({
    write(chunk, _enc, cb) {
      output += String(chunk);
      cb();
    },
  });
  // Minimal ui stub — printHelp uses head() and dim() for styling only.
  const ui = {
    head: (s) => s,
    dim: (s) => s,
    info: (s) => s,
    err: (s) => s,
  };
  printHelp(stdout, ui, { version: "0.0.0-test" });
  return output;
}

// Map a registry key to the prose token that should appear in --help. Some
// keys have a 1:1 token (login, logout, doctor, billing). Subcommand groups
// (workspace, agent, grant, tenant, zombie.*) just need their family name to
// appear at least once with a subcommand example. The contract this test
// enforces is "discoverable", not "letter-perfect documentation."
const REGISTRY_TO_HELP_TOKEN = {
  login: "login",
  logout: "logout",
  doctor: "doctor",
  workspace: "workspace ",
  agent: "agent ",
  grant: "grant ",
  tenant: "tenant ",
  billing: "billing ",
  "zombie.install": "install",
  "zombie.list": "list",
  "zombie.status": "status",
  "zombie.kill": "kill",
  "zombie.stop": "stop",
  "zombie.resume": "resume",
  "zombie.delete": "delete",
  "zombie.logs": "logs",
  "zombie.events": "events",
  "zombie.steer": "steer",
  "zombie.credential": "credential",
};

test("help: every registered command surfaces in --help output", () => {
  // Dummy handlers — registerProgramCommands just maps names to fn refs;
  // we only need the keys, not callable handlers.
  const fakeHandlers = new Proxy(
    {},
    { get: (_t, _prop) => () => undefined },
  );
  const registry = registerProgramCommands(fakeHandlers);
  const help = captureHelp();

  const missing = [];
  for (const key of Object.keys(registry)) {
    const token = REGISTRY_TO_HELP_TOKEN[key];
    assert.ok(token, `registry key '${key}' has no expected HELP token — update REGISTRY_TO_HELP_TOKEN in this test`);
    if (!help.includes(token)) {
      missing.push(`${key} (expected token: '${token}')`);
    }
  }

  assert.equal(
    missing.length,
    0,
    `wired commands missing from --help:\n  ${missing.join("\n  ")}\n\nFix: extend printHelp() in zombiectl/src/program/io.js to document the missing command(s).`,
  );
});

test("help: registry has no orphan keys (every key has a token mapping in this test)", () => {
  const fakeHandlers = new Proxy(
    {},
    { get: (_t, _prop) => () => undefined },
  );
  const registry = registerProgramCommands(fakeHandlers);

  const unmapped = Object.keys(registry).filter(
    (key) => !(key in REGISTRY_TO_HELP_TOKEN),
  );

  assert.equal(
    unmapped.length,
    0,
    `registry keys without HELP token mapping:\n  ${unmapped.join("\n  ")}\n\nFix: when adding a command, add (a) the registry entry, (b) the printHelp line, AND (c) the REGISTRY_TO_HELP_TOKEN entry in this test.`,
  );
});
