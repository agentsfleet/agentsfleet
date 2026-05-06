import { test } from "bun:test";
import assert from "node:assert/strict";
import { parseFlags, parseGlobalArgs, normalizeApiUrl } from "../src/program/args.js";

test("parseFlags parses mixed forms: inline values, separated values, booleans, and positionals", () => {
  const parsed = parseFlags([
    "--workspace-id=ws_123",
    "--mode",
    "api",
    "run",
    "status",
    "run_123",
    "--dry-run",
  ]);

  assert.equal(parsed.options["workspace-id"], "ws_123");
  assert.equal(parsed.options.mode, "api");
  assert.equal(parsed.options["dry-run"], true);
  assert.deepEqual(parsed.positionals, ["run", "status", "run_123"]);
});

test("parseFlags treats subsequent option token as a new flag", () => {
  const parsed = parseFlags(["--json", "--no-open", "--help"]);
  assert.equal(parsed.options.json, true);
  assert.equal(parsed.options["no-open"], true);
  assert.equal(parsed.options.help, true);
  assert.deepEqual(parsed.positionals, []);
});

test("parseFlags preserves equals-containing values", () => {
  const parsed = parseFlags(["--filter=a=b=c", "--empty=", "--name", "alpha=beta"]);
  assert.equal(parsed.options.filter, "a=b=c");
  assert.equal(parsed.options.empty, "");
  assert.equal(parsed.options.name, "alpha=beta");
});

test("parseFlags keeps unicode and whitespace positionals unchanged", () => {
  const parsed = parseFlags(["--scope", "sandbox", "नमस्ते", "emoji_😀"]);
  assert.equal(parsed.options.scope, "sandbox");
  assert.deepEqual(parsed.positionals, ["नमस्ते", "emoji_😀"]);
});

test("normalizeApiUrl strips trailing slashes but keeps core URL", () => {
  assert.equal(normalizeApiUrl("https://api.example.com///"), "https://api.example.com");
  assert.equal(normalizeApiUrl("http://localhost:3000"), "http://localhost:3000");
});

test("normalizeApiUrl strips trailing slash on the production default host", () => {
  assert.equal(normalizeApiUrl("https://api.usezombie.com/"), "https://api.usezombie.com");
  assert.equal(normalizeApiUrl("https://api.usezombie.com//"), "https://api.usezombie.com");
});

test("normalizeApiUrl falls back to the production default for nullish or empty input", () => {
  assert.equal(normalizeApiUrl(null), "https://api.usezombie.com");
  assert.equal(normalizeApiUrl(undefined), "https://api.usezombie.com");
  assert.equal(normalizeApiUrl(""), "https://api.usezombie.com");
});

test("parseGlobalArgs prioritizes --api over env and forwards remaining args", () => {
  const env = { ZOMBIE_API_URL: "https://env.example" };
  const out = parseGlobalArgs(["--api", "https://flag.example/", "doctor"], env);
  assert.equal(out.global.apiUrl, "https://flag.example");
  assert.deepEqual(out.rest, ["doctor"]);
});

test("parseGlobalArgs falls back through env chain and defaults", () => {
  const outApiUrl = parseGlobalArgs(["doctor"], { API_URL: "https://api-url.example" });
  assert.equal(outApiUrl.global.apiUrl, "https://api-url.example");

  const outDefault = parseGlobalArgs(["doctor"], {});
  assert.equal(outDefault.global.apiUrl, "https://api.usezombie.com");
});

test("parseGlobalArgs uses ZOMBIE_API_URL when no flag is provided", () => {
  const out = parseGlobalArgs(["doctor"], { ZOMBIE_API_URL: "https://zombie-env.example" });
  assert.equal(out.global.apiUrl, "https://zombie-env.example");
});

test("parseGlobalArgs prefers ZOMBIE_API_URL over API_URL when both env vars are set", () => {
  const out = parseGlobalArgs(["doctor"], {
    ZOMBIE_API_URL: "https://zombie-env.example",
    API_URL: "https://api-url.example",
  });
  assert.equal(out.global.apiUrl, "https://zombie-env.example");
});

test("parseGlobalArgs falls through empty ZOMBIE_API_URL to API_URL or default", () => {
  const outFallthroughToApiUrl = parseGlobalArgs(["doctor"], {
    ZOMBIE_API_URL: "",
    API_URL: "https://api-url.example",
  });
  assert.equal(outFallthroughToApiUrl.global.apiUrl, "https://api-url.example");

  const outFallthroughToDefault = parseGlobalArgs(["doctor"], { ZOMBIE_API_URL: "" });
  assert.equal(outFallthroughToDefault.global.apiUrl, "https://api.usezombie.com");
});

test("parseGlobalArgs sets global boolean options and leaves command argv intact", () => {
  const out = parseGlobalArgs(["--json", "--no-input", "--no-open", "run", "status", "run_1"], {});
  assert.equal(out.global.json, true);
  assert.equal(out.global.noInput, true);
  assert.equal(out.global.noOpen, true);
  assert.deepEqual(out.rest, ["run", "status", "run_1"]);
});
