// registry.unit.test.js — pins the M63_006 §1+§2 invariants:
// every registered command is { name, handler, errorMap }; every
// errorMap entry is { code, message } with non-empty strings; every
// UZ-* key matches the canonical format; auth-critical routes
// include every AUTH_PRESET key.
//
// Replaces the original §4 shell audit. Coverage validation (does
// each errorMap include every UZ-* code its endpoints can return?)
// is deferred per the spec's Out of Scope until OpenAPI documents
// the full error registry.

import fs from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, test, expect } from "bun:test";

import { registerProgramCommands } from "../src/program/command-registry.js";
import { AUTH_PRESET } from "../src/lib/error-map-presets.js";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const CLI_JS_PATH = path.resolve(HERE, "../src/cli.js");

const UZ_KEY = /^UZ-[A-Z]+-[0-9]+$/;

const AUTH_CRITICAL_ROUTES = ["login", "logout", "workspace", "doctor"];

function buildHandlers() {
  const stub = () => 0;
  return registerProgramCommands({
    login: stub,
    logout: stub,
    workspace: stub,
    doctor: stub,
    agent: stub,
    grant: stub,
    tenant: stub,
    billing: stub,
    zombieInstall: stub,
    zombieList: stub,
    zombieStatus: stub,
    zombieKill: stub,
    zombieStop: stub,
    zombieResume: stub,
    zombieDelete: stub,
    zombieLogs: stub,
    zombieSteer: stub,
    zombieEvents: stub,
    zombieCredential: stub,
  });
}

describe("M63_006 — registry invariants", () => {
  const handlers = buildHandlers();
  const entries = Object.entries(handlers);

  test("every entry has shape { name: string, handler: function, errorMap: object }", () => {
    expect(entries.length).toBeGreaterThan(0);
    for (const [key, value] of entries) {
      expect(typeof value.name).toBe("string");
      expect(value.name.length).toBeGreaterThan(0);
      expect(value.name).toBe(key);
      expect(typeof value.handler).toBe("function");
      expect(value.errorMap).toBeDefined();
      expect(typeof value.errorMap).toBe("object");
      expect(value.errorMap).not.toBeNull();
    }
  });

  test("every UZ-* key matches /^UZ-[A-Z]+-[0-9]+$/", () => {
    const violations = [];
    for (const [routeKey, value] of entries) {
      for (const code of Object.keys(value.errorMap)) {
        if (!UZ_KEY.test(code)) {
          violations.push(`${routeKey} → ${code}`);
        }
      }
    }
    expect(violations).toEqual([]);
  });

  test("every errorMap entry is { code: non-empty string, message: non-empty string }", () => {
    const violations = [];
    for (const [routeKey, value] of entries) {
      for (const [code, mapped] of Object.entries(value.errorMap)) {
        if (!mapped || typeof mapped !== "object") {
          violations.push(`${routeKey} ${code}: not an object`);
          continue;
        }
        if (typeof mapped.code !== "string" || mapped.code.length === 0) {
          violations.push(`${routeKey} ${code}: missing/empty .code`);
        }
        if (typeof mapped.message !== "string" || mapped.message.length === 0) {
          violations.push(`${routeKey} ${code}: missing/empty .message`);
        }
      }
    }
    expect(violations).toEqual([]);
  });

  test("auth-critical routes include every AUTH_PRESET key", () => {
    const authKeys = Object.keys(AUTH_PRESET);
    const gaps = [];
    for (const route of AUTH_CRITICAL_ROUTES) {
      const map = handlers[route]?.errorMap || {};
      for (const code of authKeys) {
        if (!(code in map)) gaps.push(`${route} missing ${code}`);
      }
    }
    expect(gaps).toEqual([]);
  });
});

describe("M63_006 — cli.js teardown invariants", () => {
  test("cli.js no longer references ApiError or printApiError outside comments", async () => {
    const src = await fs.readFile(CLI_JS_PATH, "utf8");
    const lines = src.split("\n");
    const violations = [];
    for (let i = 0; i < lines.length; i += 1) {
      const line = lines[i];
      const trimmed = line.trim();
      if (trimmed.startsWith("//") || trimmed.startsWith("*")) continue;
      if (/\bApiError\b/.test(line)) violations.push(`line ${i + 1}: ${trimmed}`);
      if (/\bprintApiError\b/.test(line)) violations.push(`line ${i + 1}: ${trimmed}`);
    }
    expect(violations).toEqual([]);
  });

  test("legacy ZOMBIE_POSTHOG_ENABLED is dead in the live tree", async () => {
    const lib = await fs.readFile(path.resolve(HERE, "../src/lib/analytics.js"), "utf8");
    expect(lib).not.toContain("ZOMBIE_POSTHOG_ENABLED");
  });
});
