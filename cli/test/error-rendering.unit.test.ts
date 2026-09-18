// The sentence an operator reads when the daemon refuses.
//
// The daemon sends two strings: `detail`, shaped for a log, and `user_message`,
// written for a person. The CLI rendered `detail`, so "Fleet Bundle is invalid"
// reached the terminal while "That Fleet Bundle isn't valid. It's missing
// `SKILL.md`, or has an unsafe or oversized file" was discarded. These cases
// pin which one is chosen, and that the choice degrades safely when the daemon
// sends only one of them — which older deployments do.

import { describe, test, expect } from "bun:test";

import { readProblemDetails } from "../src/lib/http.ts";
import { runCli } from "../src/cli.ts";
import { bufferStream, withAuthedStateDir, cliEnv } from "./helpers-cli-state.ts";
import { withMockApi, jsonResponse, type MockRoutes } from "./helpers-mock-api.ts";

const DETAIL = "Fleet Bundle is invalid";
const USER_MESSAGE =
  "That Fleet Bundle isn't valid. It's missing `SKILL.md`, or has an unsafe or oversized file.";
const TITLE = "Invalid Fleet Bundle";

describe("readProblemDetails — the two refusal strings", () => {
  test("carries both when the daemon sends both", () => {
    const parsed = readProblemDetails({
      error_code: "UZ-BUNDLE-001",
      detail: DETAIL,
      title: TITLE,
      user_message: USER_MESSAGE,
      request_id: "req_1",
    });
    expect(parsed.message).toBe(DETAIL);
    expect(parsed.userMessage).toBe(USER_MESSAGE);
    expect(parsed.code).toBe("UZ-BUNDLE-001");
    expect(parsed.requestId).toBe("req_1");
  });

  test("leaves userMessage undefined when the daemon sends none", () => {
    // Older deployments answer without `user_message`. The renderer falls back
    // to `message`, so this must be absent rather than an empty string, which
    // would render a blank failure line.
    const parsed = readProblemDetails({ detail: DETAIL, error_code: "UZ-X-001" });
    expect(parsed.userMessage).toBeUndefined();
    expect(parsed.message).toBe(DETAIL);
  });

  test("ignores a non-string user_message rather than rendering its shape", () => {
    const parsed = readProblemDetails({ detail: DETAIL, user_message: { text: "nope" } });
    expect(parsed.userMessage).toBeUndefined();
  });

  test("falls back to title when the daemon sends no detail", () => {
    const parsed = readProblemDetails({ title: TITLE });
    expect(parsed.message).toBe(TITLE);
  });

  test("a non-object body yields an empty reading, not a throw", () => {
    expect(readProblemDetails(null)).toEqual({});
    expect(readProblemDetails("plain text")).toEqual({});
  });
});

describe("rendered failure — which string reaches the terminal", () => {
  const WS_ID = "01900000-0000-7000-8000-00000067e210";
  const LIBRARIES = `/v1/workspaces/${WS_ID}/fleet-libraries`;

  const scope = <T>(fn: (stateDir: string) => Promise<T>): Promise<T> =>
    withAuthedStateDir({ workspaceId: WS_ID, sessionId: "sess_err" }, fn);

  const refuseWith = (status: number, body: Record<string, unknown>): MockRoutes => ({
    [`POST ${LIBRARIES}`]: () => jsonResponse(status, body),
  });

  const addAndReadStderr = async (routes: MockRoutes): Promise<{ code: number; text: string }> => {
    let captured = { code: 0, text: "" };
    await withMockApi(routes, async (apiUrl) => {
      const out = bufferStream();
      const err = bufferStream();
      const code = await runCli(["library", "add", "--github", "owner/repo"], {
        stdout: out.stream,
        stderr: err.stream,
        env: cliEnv({ AGENTSFLEET_API_URL: apiUrl }),
      });
      captured = { code, text: err.read() };
    });
    return captured;
  };

  test("prefers the daemon's human sentence over its log detail", async () => {
    await scope(async () => {
      const { code, text } = await addAndReadStderr(
        refuseWith(400, { error_code: "UZ-BUNDLE-001", detail: DETAIL, user_message: USER_MESSAGE }),
      );
      expect(code).toBe(3);
      expect(text).toContain(USER_MESSAGE);
      expect(text).not.toContain(DETAIL);
    });
  });

  test("falls back to the log detail when no human sentence was sent", async () => {
    await scope(async () => {
      const { code, text } = await addAndReadStderr(
        refuseWith(400, { error_code: "UZ-BUNDLE-001", detail: DETAIL }),
      );
      expect(code).toBe(3);
      expect(text).toContain(DETAIL);
    });
  });

  test("a 5xx refusal also renders the human sentence, with the retry suggestion", async () => {
    await scope(async () => {
      // The server-fault arm builds its own suggestion, so it is a separate
      // construction site from the 4xx arm and picks the detail separately.
      const { code, text } = await addAndReadStderr(
        refuseWith(503, {
          error_code: "UZ-UNAVAILABLE-001",
          detail: "upstream unavailable",
          user_message: "The service is briefly unavailable. Try again in a moment.",
        }),
      );
      expect(code).toBe(3);
      expect(text).toContain("The service is briefly unavailable.");
      expect(text).toContain("request_id");
    });
  });
});
