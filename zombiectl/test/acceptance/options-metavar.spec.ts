/**
 * End-to-end coverage that every `[--option <value>]` round-trips.
 *
 * Three contracts per option-bearing command:
 *
 *   1. `--help` documents the option with the angle-bracket `<metavar>`
 *      convention (e.g. `--limit <n>`, `--cursor <token>`).
 *   2. The validator rejects bad input with a clear stderr stem
 *      (commander exits 2 on parse failure).
 *   3. A valid value flows end-to-end through to where it should
 *      appear on the wire — captured by an in-memory stub HTTP server
 *      that records `(method, url, body)` for every request the CLI
 *      issues. We do not assert server semantics; we assert that the
 *      operator's input reached the request the handler built.
 *
 * Captain's directive: "as long as we explain to the user we are fine.
 * and those options must be tested end to end and work". This spec is
 * the explanation surface (--help) + the verification surface
 * (validator + wire request).
 */

import { describe, it, beforeAll, afterAll } from "bun:test";
import assert from "node:assert/strict";
import http from "node:http";

import { runZombiectl, composeEnv } from "./fixtures/cli.js";
import { makeStubbedStateDir, type StubbedStateDir } from "./fixtures/state-dir.ts";
import type { AddressInfo } from "node:net";

// Any valid uuidv7 satisfies parseIdOption. Reusing the example string
// the validator embeds in its error message keeps the test fixture in
// lock-step with the user-visible stem.
const FIXTURE_UUIDV7 = "0192a3b4-c5d6-7e8f-9012-345678901234";
const FIXTURE_UUIDV7_B = "0192a3b4-c5d6-7e8f-9012-345678901235";

interface CapturedRequest {
  readonly method: string | undefined;
  readonly url: string;
  readonly body: string;
}

interface CapturingStub {
  readonly baseUrl: string;
  readonly captured: CapturedRequest[];
  close(): Promise<void>;
}

/**
 * Tiny capturing HTTP stub. Records every `(method, url, body)` it
 * receives and replies from a route table. Anything unmatched returns
 * an empty `{items: []}` envelope so the CLI keeps walking — the spec
 * cares about the captured request, not the rendered response.
 */
function startCapturingStub(routes: Readonly<Record<string, unknown>> = {}): Promise<CapturingStub> {
  const captured: CapturedRequest[] = [];
  const server = http.createServer((req, res) => {
    let body = "";
    req.on("data", (c: Buffer | string) => { body += c.toString("utf8"); });
    req.on("end", () => {
      const reqUrl = req.url ?? "";
      captured.push({ method: req.method, url: reqUrl, body });
      const key = `${req.method} ${reqUrl.split("?")[0] ?? ""}`;
      const entry = routes[key];
      const payload = entry !== undefined ? JSON.stringify(entry) : JSON.stringify({ items: [] });
      res.writeHead(200, {
        "content-type": "application/json",
        "content-length": Buffer.byteLength(payload),
      });
      res.end(payload);
    });
  });
  return new Promise<CapturingStub>((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const address = server.address() as AddressInfo;
      resolve({
        baseUrl: `http://127.0.0.1:${address.port}`,
        captured,
        close: () => new Promise<void>((r) => server.close(() => r())),
      });
    });
  });
}

let stateDir: StubbedStateDir;
let workspaceUuid: string;

beforeAll(async () => {
  // Seed the state dir with a uuidv7 workspace id so handlers that
  // build paths from `workspaces.current_workspace_id` produce URLs
  // an operator would actually see — keeps the captured-request
  // assertions readable.
  workspaceUuid = FIXTURE_UUIDV7;
  stateDir = await makeStubbedStateDir({ workspaceId: workspaceUuid });
});

afterAll(async () => {
  if (stateDir) await stateDir.cleanup();
});

function helpEnv(): Record<string, string> {
  return composeEnv({ NO_COLOR: "1" });
}

function runEnv(extra?: Record<string, string>): Record<string, string> {
  return composeEnv({
    ZOMBIE_STATE_DIR: stateDir.dir,
    NO_COLOR: "1",
    ...(extra ?? {}),
  });
}

describe("--help bodies use angle-bracket metavar convention", () => {
  type HelpCase = readonly [string, ReadonlyArray<string>, ReadonlyArray<string>];
  const cases: ReadonlyArray<HelpCase> = [
    ["zombiectl list --help",                 ["list", "--help"],                 ["--limit <n>", "--cursor <token>", "--workspace-id <id>"]],
    ["zombiectl logs --help",                 ["logs", "--help"],                 ["--limit <n>", "--cursor <token>", "--zombie <id>"]],
    ["zombiectl events --help",               ["events", "--help"],               ["--limit <n>", "--since <when>", "--actor <glob>", "--cursor <token>"]],
    ["zombiectl install --help",              ["install", "--help"],              ["--from <path>"]],
    ["zombiectl login --help",                ["login", "--help"],                ["--timeout-sec <n>", "--poll-ms <n>"]],
    ["zombiectl billing show --help",         ["billing", "show", "--help"],      ["--limit <n>", "--cursor <token>"]],
    ["zombiectl agent add --help",            ["agent", "add", "--help"],         ["--workspace <id>", "--zombie <id>", "--name <name>"]],
    ["zombiectl tenant provider add --help",  ["tenant", "provider", "add", "--help"], ["--credential <name>", "--model <name>"]],
  ];

  for (const [name, argv, metavars] of cases) {
    it(`${name} advertises ${metavars.join(", ")}`, async () => {
      const result = await runZombiectl(argv, { env: helpEnv() });
      assert.equal(result.code, 0, `expected exit 0; stderr=${result.stderr}`);
      for (const m of metavars) {
        assert.ok(
          result.stdout.includes(m),
          `expected '${m}' in --help body; stdout=\n${result.stdout}`,
        );
      }
    });
  }
});

describe("validators reject invalid values with clear error stem", () => {
  type ValidatorCase = readonly [string, ReadonlyArray<string>, RegExp];
  // The CLI surfaces commander's "option '--X <v>' argument 'Y' is invalid.
  // <stem>" line on stderr and exits non-zero. The stem is the contract
  // we pin here. Exit code is currently 1 for commander.invalidArgument
  // (commander wraps the InvalidArgumentError and exits 1); a separate
  // cli.ts hygiene PR can map it to POSIX 2 by extending
  // COMMANDER_USAGE_CODES.
  const cases: ReadonlyArray<ValidatorCase> = [
    // parseIntOption rejections (commander wraps as "option '--x <n>' argument 'V' is invalid. <stem>")
    ["list --limit 0",        ["list", "--limit", "0"],          /must be ≥ 1/],
    ["list --limit abc",      ["list", "--limit", "abc"],        /must be an integer/],
    ["list --limit 9999",     ["list", "--limit", "9999"],       /must be ≤ 200/],
    ["billing show --limit 9999", ["billing", "show", "--limit", "9999"], /must be ≤ 100/],
    ["logs --limit 9999",     ["logs", "--limit", "9999"],       /must be ≤ 500/],
    ["events <id> --limit 9999", ["events", FIXTURE_UUIDV7, "--limit", "9999"], /must be ≤ 500/],
    ["login --timeout-sec 0", ["login", "--timeout-sec", "0"],   /must be ≥ 1/],
    ["login --poll-ms 999999", ["login", "--poll-ms", "999999"], /must be ≤ 60000/],
    // parseIdOption rejections (uuidv7 enforced)
    ["agent add --workspace not-a-uuid", ["agent", "add", "--workspace", "not-a-uuid", "--zombie", FIXTURE_UUIDV7], /uuidv7 format/],
    ["agent add --zombie not-a-uuid",    ["agent", "add", "--workspace", FIXTURE_UUIDV7, "--zombie", "not-a-uuid"], /uuidv7 format/],
    ["list --workspace-id not-a-uuid",   ["list", "--workspace-id", "not-a-uuid"], /uuidv7 format/],
  ];

  for (const [name, argv, stemRe] of cases) {
    it(`${name} → non-zero exit + ${stemRe}`, async () => {
      const result = await runZombiectl(argv, { env: runEnv() });
      assert.notEqual(result.code, 0, `expected non-zero exit; got 0; stderr=${result.stderr}`);
      assert.match(result.stderr, stemRe, `expected stderr to match ${stemRe}; stderr=${result.stderr}`);
    });
  }
});

describe("option values flow end-to-end into the wire request", () => {
  let stub: CapturingStub | null = null;
  beforeAll(async () => { stub = await startCapturingStub(); });
  afterAll(async () => { if (stub) await stub.close(); });

  function requireStub(): CapturingStub {
    if (!stub) throw new Error("capturing stub not initialised");
    return stub;
  }
  function clear(): void { requireStub().captured.length = 0; }
  function apiEnv(extra?: Record<string, string>): Record<string, string> {
    return runEnv({ ZOMBIE_API_URL: requireStub().baseUrl, ...(extra ?? {}) });
  }

  it("zombie list --limit 25 --cursor abc123 → GET .../zombies?cursor=abc123&limit=25", async () => {
    clear();
    const result = await runZombiectl(["list", "--limit", "25", "--cursor", "abc123", "--json"], { env: apiEnv() });
    assert.equal(result.code, 0, `stderr=${result.stderr}`);
    const captured = requireStub().captured;
    const hit = captured.find((c) => c.url.includes("/zombies") && c.method === "GET");
    assert.ok(hit, `no /zombies GET captured: ${JSON.stringify(captured)}`);
    assert.match(hit.url, /[?&]limit=25(&|$)/);
    assert.match(hit.url, /[?&]cursor=abc123(&|$)/);
  });

  it("zombie logs <id> --limit 50 → GET .../zombies/<id>/events?limit=50", async () => {
    clear();
    const result = await runZombiectl(["logs", FIXTURE_UUIDV7, "--limit", "50", "--json"], { env: apiEnv() });
    assert.equal(result.code, 0, `stderr=${result.stderr}`);
    const captured = requireStub().captured;
    const hit = captured.find((c) => c.url.includes(`/zombies/${FIXTURE_UUIDV7}/events`));
    assert.ok(hit, `no per-zombie events GET captured: ${JSON.stringify(captured)}`);
    assert.match(hit.url, /[?&]limit=50(&|$)/);
  });

  it("events <id> --limit 100 --since 2h --actor 'steer:*' → GET ...events?actor=&since=&limit=", async () => {
    clear();
    const result = await runZombiectl(
      ["events", FIXTURE_UUIDV7, "--limit", "100", "--since", "2h", "--actor", "steer:*", "--json"],
      { env: apiEnv() },
    );
    assert.equal(result.code, 0, `stderr=${result.stderr}`);
    const captured = requireStub().captured;
    const hit = captured.find((c) => c.url.includes(`/zombies/${FIXTURE_UUIDV7}/events`));
    assert.ok(hit, `no per-zombie events GET captured: ${JSON.stringify(captured)}`);
    assert.match(hit.url, /[?&]limit=100(&|$)/);
    assert.match(hit.url, /[?&]since=2h(&|$)/);
    assert.match(hit.url, /[?&]actor=steer(%3A|:)\*/);
  });

  it("billing show --limit 5 --cursor xyz → GET .../billing/charges?limit=10&cursor=xyz (limit doubled)", async () => {
    clear();
    const result = await runZombiectl(["billing", "show", "--limit", "5", "--cursor", "xyz", "--json"], { env: apiEnv() });
    assert.equal(result.code, 0, `stderr=${result.stderr}`);
    const captured = requireStub().captured;
    const charges = captured.find((c) => c.url.includes("/billing/charges"));
    assert.ok(charges, `no billing charges GET captured: ${JSON.stringify(captured)}`);
    // billing.ts doubles the limit (each event has up to 2 charge rows)
    // and forwards the operator's cursor verbatim. We assert both.
    assert.match(charges.url, /[?&]limit=10(&|$)/);
    assert.match(charges.url, /[?&]cursor=xyz(&|$)/);
  });

  it("agent add --workspace <uuid7> --zombie <uuid7> --name fred → POST body has name=fred", async () => {
    clear();
    const result = await runZombiectl(
      ["agent", "add", "--workspace", workspaceUuid, "--zombie", FIXTURE_UUIDV7_B, "--name", "fred", "--json"],
      { env: apiEnv() },
    );
    assert.equal(result.code, 0, `stderr=${result.stderr}`);
    const captured = requireStub().captured;
    const post = captured.find((c) => c.method === "POST" && c.url.includes("/agent-keys"));
    assert.ok(post, `no agent-keys POST captured: ${JSON.stringify(captured)}`);
    assert.ok(post.url.includes(`/workspaces/${workspaceUuid}/agent-keys`), `wrong workspace in URL: ${post.url}`);
    const body = JSON.parse(post.body) as { name?: string; zombie_id?: string };
    assert.equal(body.name, "fred", `expected name=fred in POST body; body=${post.body}`);
    assert.equal(body.zombie_id, FIXTURE_UUIDV7_B, `expected zombie_id in POST body; body=${post.body}`);
  });

  it("tenant provider add --credential keyname --model gpt-x → PUT body has credential + model", async () => {
    clear();
    const result = await runZombiectl(
      ["tenant", "provider", "add", "--credential", "keyname", "--model", "gpt-x", "--json"],
      { env: apiEnv() },
    );
    assert.equal(result.code, 0, `stderr=${result.stderr}`);
    const captured = requireStub().captured;
    const put = captured.find((c) => c.method === "PUT" && c.url.includes("/tenants/me/provider"));
    assert.ok(put, `no provider PUT captured: ${JSON.stringify(captured)}`);
    const body = JSON.parse(put.body) as { credential_ref?: string; model?: string };
    assert.equal(body.credential_ref, "keyname", `expected credential_ref=keyname in body; body=${put.body}`);
    assert.equal(body.model, "gpt-x", `expected model=gpt-x in body; body=${put.body}`);
  });
});

describe("non-wire option values reach the handler", () => {
  // install --from is handled by loadSkillFromPath in src/skills/loader.js.
  // A non-existent path produces ERR_PATH_NOT_FOUND that names the resolved
  // path — proves the validator's parsePathOption ran AND the handler
  // observed it.
  it("install --from /nonexistent/marker-7b → stderr names the resolved path", async () => {
    const result = await runZombiectl(
      ["install", "--from", "/nonexistent/marker-7b"],
      { env: runEnv({ ZOMBIE_API_URL: "http://127.0.0.1:1" }) },
    );
    assert.notEqual(result.code, 0, `expected non-zero exit; stdout=${result.stdout}`);
    const combined = `${result.stdout}\n${result.stderr}`;
    assert.match(combined, /marker-7b/, `expected --from path in error output; combined=${combined}`);
  });

  // login --timeout-sec + --poll-ms drive a poll loop. Bare integer
  // pin: --timeout-sec 1 should produce wall-clock exit ≤ 5s (vs the
  // 300s default). We exercise both options together — the timeout
  // value is the falsifiable assertion; --poll-ms only has to parse.
  it("login --no-open --no-input --timeout-sec 1 --poll-ms 250 exits within 5s", async () => {
    const stub = await startCapturingStub({
      "POST /v1/auth/sessions": {
        session_id: "sess_metavar_test",
        login_url: "http://127.0.0.1:65535/cli-auth/stub",
        status: "pending",
      },
      "GET /v1/auth/sessions/sess_metavar_test": { status: "pending" },
    });
    try {
      const t0 = Date.now();
      const result = await runZombiectl(
        ["login", "--no-open", "--no-input", "--timeout-sec", "1", "--poll-ms", "250"],
        { env: composeEnv({ ZOMBIE_API_URL: stub.baseUrl, ZOMBIE_STATE_DIR: stateDir.dir, NO_COLOR: "1" }), timeoutMs: 15_000 },
      );
      const elapsed = Date.now() - t0;
      assert.notEqual(result.code, 0, `expected non-zero (timeout) exit; stdout=${result.stdout}`);
      assert.ok(elapsed < 5_000, `expected exit within 5s, took ${elapsed}ms`);
    } finally {
      await stub.close();
    }
  });
});
