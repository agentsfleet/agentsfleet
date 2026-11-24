// A decision the daemon refuses reaches the caller as the one structured error
// every command renders: its code, the daemon's sentence and a suggestion.

import { describe, expect, test } from "bun:test";
import { Cause, Effect, Exit, Layer, Option, Redacted } from "effect";
import { approvalsApproveEffectFromArgs, approvalsDenyEffectFromArgs } from "../src/commands/approvals/decide.ts";
import { ServerError, type CliError } from "../src/errors/index.ts";
import { renderCliError } from "../src/lib/cli-error-render.ts";
import { exitCodeForFailure } from "../src/program/entry/exit-code.ts";
import { CliConfig } from "../src/services/config.ts";
import { Credentials } from "../src/services/credentials.ts";
import { httpClientLayer } from "../src/services/http-client.ts";
import { Output } from "../src/services/output.ts";
import { Workspaces } from "../src/services/workspaces.ts";
import { outputDouble } from "./helpers-output-double.ts";

const WS_ID = "01910000-0000-7000-8000-0000000a9901";
const GATE_ID = "01910000-0000-7000-8000-0000000a9902";
const API_URL = "https://api.decide-test.local";
const PERSON_REQUIRED = "Approving access needs a signed-in person; an API key cannot approve it";

interface Captured {
  readonly sent: string[];
  readonly errors: unknown[];
  readonly out: unknown[];
}

const layers = (captured: Captured, answer: Response) => Layer.mergeAll(
  Layer.provide(httpClientLayer, Layer.succeed(CliConfig, {
    apiUrl: API_URL, dashboardUrl: API_URL, accessToken: Option.none(), jsonMode: true, noOpen: true,
    telemetryPosthogKey: "", telemetryPosthogHost: API_URL,
    fetchImpl: async (url: string | URL | Request, init?: RequestInit) => {
      captured.sent.push(`${init?.method ?? "GET"} ${new URL(String(url)).pathname}`);
      return answer;
    },
  })),
  Layer.succeed(CliConfig, {
    apiUrl: API_URL, dashboardUrl: API_URL, accessToken: Option.none(), jsonMode: true, noOpen: true,
    telemetryPosthogKey: "", telemetryPosthogHost: API_URL,
  }),
  Layer.succeed(Credentials, {
    getAccessToken: Effect.succeed(Option.some(Redacted.make("agt_t_machine"))),
    snapshot: Effect.succeed({ accessToken: Option.none(), savedAt: null, sessionId: null, apiUrl: null, credentialId: null }),
    saveAccessToken: () => Effect.void,
    clearAccessToken: Effect.void,
  }),
  Layer.succeed(Workspaces, { load: Effect.succeed({ current_workspace_id: WS_ID, items: [] }), save: () => Effect.void }),
  Layer.succeed(Output, {
    ...outputDouble({ jsonMode: true }),
    printJsonErr: (payload) => Effect.sync(() => { captured.errors.push(payload); }),
    success: (_message, data) => Effect.sync(() => { captured.out.push(data); }),
  }),
);

const forbidden = () => Response.json(
  { error_code: "UZ-AUTH-001", detail: "person identity required", user_message: PERSON_REQUIRED, request_id: "req_decide" },
  { status: 403 },
);

const decide = async (effect: Effect.Effect<void, CliError, never>) => {
  const exit = await Effect.runPromiseExit(effect);
  if (Exit.isSuccess(exit)) throw new Error("expected the decision to be refused");
  return { error: Cause.squash(exit.cause), code: exitCodeForFailure(exit.cause) };
};

describe("fleet approvals approve and deny", () => {
  test("fleet approvals approve prints the refusal as JSON", async () => {
    const captured: Captured = { sent: [], errors: [], out: [] };
    const provided = layers(captured, forbidden());
    const refused = await decide(approvalsApproveEffectFromArgs(GATE_ID).pipe(Effect.provide(provided)));
    expect(refused.error).toBeInstanceOf(ServerError);
    expect(refused.code).toBe(3);
    await Effect.runPromise(renderCliError(refused.error as CliError).pipe(Effect.provide(provided)));
    expect(captured.errors).toEqual([{ error: {
      code: "UZ-AUTH-001",
      message: PERSON_REQUIRED,
      suggestion: "re-authenticate with `agentsfleet login`; if you logged into a different server, check the target API URL (--api / AGENTSFLEET_API_URL)",
      request_id: "req_decide",
    } }]);
    expect(captured.sent).toEqual([`POST /v1/workspaces/${WS_ID}/approvals/${GATE_ID}/approve`]);
    expect(captured.out).toEqual([]);
  });

  test("a refused deny is the same structured error, on its own route", async () => {
    const captured: Captured = { sent: [], errors: [], out: [] };
    const refused = await decide(approvalsDenyEffectFromArgs(GATE_ID).pipe(Effect.provide(layers(captured, forbidden()))));
    expect(refused.error).toMatchObject({ code: "UZ-AUTH-001", status: 403, requestId: "req_decide" });
    expect(captured.sent).toEqual([`POST /v1/workspaces/${WS_ID}/approvals/${GATE_ID}/deny`]);
  });

  test("an approval the daemon accepts prints the decision it recorded", async () => {
    const captured: Captured = { sent: [], errors: [], out: [] };
    const recorded = { gate_id: GATE_ID, action_id: "act_1", outcome: "approved", resolved_at: "2026-11-25T10:00:00Z", resolved_by: "user_1" };
    await Effect.runPromise(approvalsApproveEffectFromArgs(GATE_ID)
      .pipe(Effect.provide(layers(captured, Response.json(recorded)))));
    expect(captured.out).toEqual([recorded]);
    expect(captured.errors).toEqual([]);
  });
});
