// error-matrix unit test — walks the status × error_code × json-mode
// matrix through runCommand and asserts the (exit code, rendered code,
// analytics error_code) tuple. The wrapper renders directly to
// stderr via writeLine/printJson; we capture both.

import { test } from "bun:test";
import assert from "node:assert/strict";
import { ApiError } from "../src/lib/http.ts";
import { runCommand, type Handler } from "../src/lib/run-command.ts";

// Handler-side only reads `.write`; full WritableStream is overkill.
const STDERR_STUB = { write: () => true } as unknown as NodeJS.WritableStream;

interface MatrixCase {
  name: string;
  handler: Handler;
  expectExit: number;
  expectAnalytics: string;
  expectCode: string | null;
}

const CASES: readonly MatrixCase[] = [
  { name: "200 happy",        handler: () => 0,                                                        expectExit: 0, expectAnalytics: "cli_command_finished", expectCode: null },
  { name: "400 UZ-VAL-001",   handler: () => { throw new ApiError("bad input", { status: 400, code: "UZ-VALIDATION-001" }); },  expectExit: 1, expectAnalytics: "cli_error", expectCode: "UZ-VALIDATION-001" },
  { name: "401 UZ-AUTH-001",  handler: () => { throw new ApiError("nope", { status: 401, code: "UZ-AUTH-001" }); },             expectExit: 1, expectAnalytics: "cli_error", expectCode: "UZ-AUTH-001" },
  { name: "403 UZ-AUTHZ-001", handler: () => { throw new ApiError("forbidden", { status: 403, code: "UZ-AUTHZ-001" }); },       expectExit: 1, expectAnalytics: "cli_error", expectCode: "UZ-AUTHZ-001" },
  { name: "404 UZ-NF-001",    handler: () => { throw new ApiError("not found", { status: 404, code: "UZ-NF-001" }); },          expectExit: 1, expectAnalytics: "cli_error", expectCode: "UZ-NF-001" },
  { name: "408 TIMEOUT",      handler: () => { throw new ApiError("timed out", { status: 408, code: "TIMEOUT" }); },            expectExit: 1, expectAnalytics: "cli_error", expectCode: "TIMEOUT" },
  { name: "429 RATE_LIMITED", handler: () => { throw new ApiError("slow down", { status: 429, code: "RATE_LIMITED" }); },       expectExit: 1, expectAnalytics: "cli_error", expectCode: "RATE_LIMITED" },
  { name: "500 HTTP_500",     handler: () => { throw new ApiError("oops", { status: 500, code: "HTTP_500" }); },                expectExit: 1, expectAnalytics: "cli_error", expectCode: "HTTP_500" },
  { name: "503 HTTP_503",     handler: () => { throw new ApiError("blip", { status: 503, code: "HTTP_503" }); },                expectExit: 1, expectAnalytics: "cli_error", expectCode: "HTTP_503" },
  { name: "fetch failed",     handler: () => { throw new TypeError("fetch failed"); },                                          expectExit: 1, expectAnalytics: "cli_error", expectCode: "API_UNREACHABLE" },
  { name: "unknown throw",    handler: () => { throw new Error("kaboom"); },                                                    expectExit: 1, expectAnalytics: "cli_error", expectCode: "UNEXPECTED" },
];

const JSON_MODES = [false, true] as const;

interface TrackedEvent {
  event: string;
  props: Record<string, unknown>;
}

interface JsonErrorPayload {
  error: { code?: string; message?: string };
}

for (const c of CASES) {
  for (const jsonMode of JSON_MODES) {
    test(`error matrix: ${c.name} (json_mode=${jsonMode})`, async () => {
      const events: TrackedEvent[] = [];
      const lines: string[] = [];
      const jsonPayloads: JsonErrorPayload[] = [];
      const trackCliEvent = (
        _client: unknown,
        _id: string | null | undefined,
        event: string,
        props?: Record<string, unknown>,
      ): void => { events.push({ event, props: props ?? {} }); };
      const writeLine = (_stream: NodeJS.WritableStream, line = ""): void => { lines.push(line); };
      const printJson = (_stream: NodeJS.WritableStream, value: unknown): void => {
        jsonPayloads.push(value as JsonErrorPayload);
      };
      const ui = { err: (s: string) => s };

      const code = await runCommand({
        name: "matrix",
        handler: c.handler,
        ctx: { stderr: STDERR_STUB, jsonMode, apiUrl: "http://api.example.com" },
        deps: { trackCliEvent, writeLine, printJson, ui },
      });

      assert.equal(code, c.expectExit, `exit code for ${c.name}`);
      const last = events.at(-1);
      assert.ok(last, `expected analytics event for ${c.name}`);
      assert.equal(last.event, c.expectAnalytics, `analytics event for ${c.name}`);
      assert.equal(last.props["json_mode"], String(jsonMode));
      if (c.expectCode != null) {
        assert.equal(last.props["error_code"], c.expectCode, `analytics error_code for ${c.name}`);
        if (jsonMode) {
          assert.equal(jsonPayloads.length, 1, `json payload count for ${c.name}`);
          assert.equal(jsonPayloads[0]?.error.code, c.expectCode, `json error.code for ${c.name}`);
        } else {
          // Human mode: ApiError → "error: <code> <message>"; plain → bare message.
          const found = lines.some((l) => l.includes(c.expectCode as string));
          if (c.expectCode.startsWith("UZ-") || c.expectCode === "TIMEOUT" || c.expectCode === "RATE_LIMITED" || c.expectCode.startsWith("HTTP_")) {
            assert.ok(found, `expected human-mode line containing ${c.expectCode} in ${c.name}, got ${JSON.stringify(lines)}`);
          }
        }
      }
    });
  }
}
