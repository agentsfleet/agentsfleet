// `fleet triggers list|delete`: what wakes a fleet from outside, read and
// deleted by a person.

import { describe, expect, test } from "bun:test";
import { runCli } from "../src/cli.ts";
import { bufferStream, cliEnv } from "./helpers-cli-state.ts";
import { FLEET } from "./helpers-fleet.ts";
import { authedScope, WS_ID } from "./helpers-fleet-steer.ts";
import { jsonResponse, withMockApi, type MockRoutes } from "./helpers-mock-api.ts";

const FLEETS = `/v1/workspaces/${WS_ID}/fleets`;
const TRIGGERS = `${FLEETS}/${FLEET.id}/triggers`;
const WEBHOOK_ID = "01910000-0000-7000-8000-0000000b1001";
const APP_ID = "01910000-0000-7000-8000-0000000b1002";
const MENTION_ID = "01910000-0000-7000-8000-0000000b1003";
const CREATED_AT = "2026-11-25T10:00:00Z";
const HOOK_URL = `https://api.agentsfleet.net/v1/webhooks/${FLEET.id}/sentry`;
const PAGE = { items: [
  { id: WEBHOOK_ID, kind: "webhook", source: "sentry", events: ["issue.created"], url: HOOK_URL, created_at: CREATED_AT },
  { id: APP_ID, kind: "app_event", source: "github", repositories: ["org/app"], events: ["pull_request"], created_at: CREATED_AT },
  { id: MENTION_ID, kind: "mention", source: "slack", channel: "C0INCIDENTS", events: null, url: null, created_at: CREATED_AT },
] };

const run = (argv: ReadonlyArray<string>, routes: MockRoutes, terminal = true) =>
  authedScope(() => withMockApi(routes, async (apiUrl, calls) => {
    const out = bufferStream();
    if (terminal) out.stream.isTTY = true;
    const err = bufferStream();
    const code = await runCli([...argv], { stdout: out.stream, stderr: err.stream, env: cliEnv({ AGENTSFLEET_API_URL: apiUrl, NO_COLOR: "1" }) });
    return { code, out: out.read(), err: err.read(), sent: calls.map((call) => `${call.method} ${call.path}`) };
  }));

describe("fleet triggers list", () => {
  test("lists each trigger with what it wakes on", async () => {
    const result = await run(["fleet", "triggers", "list", FLEET.id], { [`GET ${TRIGGERS}`]: () => jsonResponse(200, PAGE) });
    expect(result.code).toBe(0);
    expect(result.sent).toEqual([`GET ${TRIGGERS}`]);
    for (const cell of [WEBHOOK_ID, "webhook", "sentry", "issue.created", HOOK_URL,
      APP_ID, "app_event", "org/app, pull_request", MENTION_ID, "mention", "C0INCIDENTS"]) {
      expect(result.out).toContain(cell);
    }
  });

  test("a fleet with no trigger says how one is added", async () => {
    const result = await run(["fleet", "triggers", "list", FLEET.id], { [`GET ${TRIGGERS}`]: () => jsonResponse(200, { items: [] }) });
    expect(result.code).toBe(0);
    expect(result.out).toContain("No triggers. Ask the fleet to wake on a webhook, a GitHub event or a Slack mention, and it adds one.");
  });

  test("a stdout that is not a terminal gets the daemon's list verbatim", async () => {
    const result = await run(["fleet", "triggers", "list", FLEET.id], { [`GET ${TRIGGERS}`]: () => jsonResponse(200, PAGE) }, false);
    expect(result.code).toBe(0);
    expect(JSON.parse(result.out)).toEqual(PAGE);
  });

  test.each([
    ["a row with no id", { items: [{ kind: "webhook", source: "sentry", created_at: CREATED_AT }] }],
    ["an unreadable time", { items: [{ ...PAGE.items[0], created_at: "yesterday" }] }],
    ["no items", {}],
  ] as const)("a list with %s is refused rather than half-shown", async (_case, body) => {
    const result = await run(["fleet", "triggers", "list", FLEET.id], { [`GET ${TRIGGERS}`]: () => jsonResponse(200, body) });
    expect(result.code).toBe(3);
    expect(result.err).toContain("TRIGGER_RESPONSE_INVALID");
    expect(result.out).toBe("");
  });

  test("a malformed fleet id is refused before any request", async () => {
    const result = await run(["fleet", "triggers", "list", "not-a-uuid"], {});
    expect(result.code).toBe(4);
    expect(result.sent.filter((call) => call.includes("/fleets"))).toEqual([]);
  });
});

describe("fleet triggers delete", () => {
  const deleted: MockRoutes = { [`DELETE ${TRIGGERS}/${WEBHOOK_ID}`]: () => new Response(null, { status: 204 }) };

  test("deletes the one trigger and says it no longer wakes the fleet", async () => {
    const result = await run(["fleet", "triggers", "delete", FLEET.id, WEBHOOK_ID], deleted);
    expect(result.code).toBe(0);
    expect(result.sent).toEqual([`DELETE ${TRIGGERS}/${WEBHOOK_ID}`]);
    expect(result.out).toContain(`Deleted trigger ${WEBHOOK_ID}; it no longer wakes the fleet.`);
  });

  test("--json reports the deletion as a record", async () => {
    const result = await run(["--json", "fleet", "triggers", "delete", FLEET.id, WEBHOOK_ID], deleted);
    expect(result.code).toBe(0);
    expect(JSON.parse(result.out)).toEqual({ deleted: true, trigger_id: WEBHOOK_ID });
  });

  test("a trigger the fleet does not have is the daemon's refusal", async () => {
    const result = await run(["fleet", "triggers", "delete", FLEET.id, APP_ID], {
      [`DELETE ${TRIGGERS}/${APP_ID}`]: () => jsonResponse(404, { error_code: "UZ-AGT-030", detail: "No such trigger", request_id: "req_trig" }),
    });
    expect(result.code).toBe(3);
    expect(result.err).toContain("UZ-AGT-030");
    expect(result.out).not.toContain("Deleted trigger");
  });

  test("a malformed trigger id is refused before any request", async () => {
    const result = await run(["fleet", "triggers", "delete", FLEET.id, "not-a-uuid"], {});
    expect(result.code).toBe(4);
    expect(result.err).toContain("trigger_id");
    expect(result.sent.filter((call) => call.includes("/fleets"))).toEqual([]);
  });
});
