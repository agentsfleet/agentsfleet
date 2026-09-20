// Shared fixtures and doubles for the steer integration suites.
//
// Extracted when fleet-steer.integration.test.ts passed the repository's 350-line cap;
// the suites that read them are unchanged.

import { Effect, Layer, Option, Redacted } from "effect";
import { Readable, Writable } from "node:stream";
import { CliConfig } from "../src/services/config.ts";
import { Credentials } from "../src/services/credentials.ts";
import { HttpClient, type HttpRequestInput } from "../src/services/http-client.ts";
import { Output, type OutputShape } from "../src/services/output.ts";
import { outputDouble } from "./helpers-output-double.ts";
import { Workspaces } from "../src/services/workspaces.ts";
import type { StreamGetCallback } from "../src/lib/sse.ts";
import { withAuthedStateDir } from "./helpers-cli-state.ts";

// Exported so the sibling error-path suite shares one source of truth for
// the fixture ids + mocked-layer config (see makeLayer below).
export const WS_ID = "01910000-0000-7000-8000-000000a6e711";
export const FLEET_ID = "01910000-0000-7000-8000-000000a67e57";
export const TOKEN = "test.jwt.token";
export const EVENT_ID = "1729874000000-abc";
export const OTHER_EVENT_ID = "1729874000000-other";
export const API_URL = "https://api.steer-test.local";
export const DASHBOARD_URL = "https://dash.steer-test.local";
// Call-order markers shared with the failure-path suite: fakes push these so
// ordering assertions read as data, not as index arithmetic.
export const CALL_STREAM_OPEN = "stream-open";
export const CALL_POST = "post-message";
// Shared across the steer suites (single declaration site).
export const POST = "POST";
export const SINGLE_MESSAGE = "go";
export const postedEvent = <T>(): T => ({ event_id: EVENT_ID } as T);

export const authedScope = <T>(fn: (stateDir: string) => Promise<T>): Promise<T> =>
  withAuthedStateDir({ workspaceId: WS_ID, sessionId: "sess_steer" }, fn);

export const streamFrom = (chunks: ReadonlyArray<string>, isTTY: boolean) => {
  const stream = Readable.from(chunks);
  Object.defineProperty(stream, "isTTY", { value: isTTY });
  return stream as unknown as import("../src/lib/repl.ts").ReplInputStream;
};

export const nullOutput = () =>
  new Writable({ write(_c, _e, cb) { cb(); } }) as unknown as
    import("../src/lib/repl.ts").ReplOutputStream;

export interface Recorder {
  readonly stdout: string[];
  readonly stderr: string[];
  readonly requests: HttpRequestInput[];
}

export const makeRecorder = (): Recorder => ({ stdout: [], stderr: [], requests: [] });

export const makeLayer = (
  rec: Recorder,
  httpReply: <T>(input: HttpRequestInput) => T = <T>() => ({ event_id: EVENT_ID } as T),
  jsonMode = false,
  outputOverrides?: Partial<OutputShape>,
) =>
  Layer.mergeAll(
    Layer.succeed(CliConfig, {
      apiUrl: API_URL,
      dashboardUrl: DASHBOARD_URL,
      accessToken: Option.none(),
      jsonMode,
      noOpen: false,
      telemetryPosthogKey: "phc_test",
      telemetryPosthogHost: "https://us.i.posthog.com",
    }),
    Layer.succeed(Credentials, {
      getAccessToken: Effect.sync(() => Option.some(Redacted.make(TOKEN))),
      snapshot: Effect.succeed({ accessToken: Option.none(), savedAt: null, sessionId: null, apiUrl: null, credentialId: null }),
      saveAccessToken: () => Effect.void,
      clearAccessToken: Effect.void,
    }),
    Layer.succeed(Workspaces, {
      load: Effect.sync(() => ({ current_workspace_id: WS_ID, items: [] })),
      save: () => Effect.void,
    }),
    Layer.succeed(HttpClient, {
      request: <T>(input: HttpRequestInput) =>
        Effect.sync(() => { rec.requests.push(input); return httpReply<T>(input); }),
    }),
    Layer.succeed(Output, {
      ...outputDouble({ jsonMode }),
      intro: (m) => Effect.sync(() => { rec.stdout.push(m); }),
      info: (m) => Effect.sync(() => { rec.stdout.push(m); }),
      success: (m, d) =>
        Effect.sync(() => { rec.stdout.push(jsonMode && d ? JSON.stringify(d) : m); }),
      warn: (m) => Effect.sync(() => { rec.stderr.push(m); }),
      error: (m) => Effect.sync(() => { rec.stderr.push(m); }),
      outro: (m) => Effect.sync(() => { rec.stdout.push(m); }),
      printJson: (p) => Effect.sync(() => { rec.stdout.push(JSON.stringify(p)); }),
      printJsonErr: (p) => Effect.sync(() => { rec.stderr.push(JSON.stringify(p)); }),
      ...outputOverrides,
    }),
  );

export const eventStream = (events: Parameters<StreamGetCallback>[0][]) =>
  async (
    _url: string,
    _headers: Record<string, string>,
    cb: StreamGetCallback,
  ): Promise<void> => {
    for (const ev of events) {
      if (cb(ev) === false) return;
    }
  };
