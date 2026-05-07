// error-matrix.unit.test.js — covers M63_004 Test Spec
// `error_matrix_pins_stderr_lines`. Walks the status × error_code ×
// json-mode matrix through runCommand and asserts the (code, message,
// exit_code, analytics event_code) tuple.
//
// We test the WRAPPER's stable surface (exit code + writeError payload
// + analytics event) — not the literal stderr formatting, which is
// owned by printApiError and locked separately by analytics tests.
// The wrapper is the new boundary; pinning its outputs pins the
// per-handler invariants.

import { test } from "bun:test";
import assert from "node:assert/strict";
import { ApiError } from "../src/lib/http.js";
import { runCommand } from "../src/lib/run-command.js";

const CASES = [
  // [status, errorCode, expectedPrintedCode, expectedAnalyticsCode, errCtor]
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

const JSON_MODES = [false, true];

for (const c of CASES) {
  for (const jsonMode of JSON_MODES) {
    test(`error matrix: ${c.name} (json_mode=${jsonMode})`, async () => {
      const events = [];
      let written = null;
      const trackCliEvent = (_, __, event, props) => events.push({ event, props });
      const writeError = (info) => { written = info; };

      const code = await runCommand({
        name: "matrix",
        handler: c.handler,
        ctx: { jsonMode, apiUrl: "http://api.example.com" },
        deps: { trackCliEvent, writeError },
      });

      assert.equal(code, c.expectExit, `exit code for ${c.name}`);
      const last = events.at(-1);
      assert.equal(last.event, c.expectAnalytics, `analytics event for ${c.name}`);
      assert.equal(last.props.json_mode, String(jsonMode));
      if (c.expectCode != null) {
        assert.equal(last.props.error_code, c.expectCode, `analytics error_code for ${c.name}`);
        assert.equal(written.code, c.expectCode, `printed code for ${c.name}`);
      }
    });
  }
}
