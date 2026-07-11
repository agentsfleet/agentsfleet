import { describe, expect, test } from "bun:test";
import { summarizeConnector, summarizeStatus } from "../src/services/connectors.ts";

describe("connector state summaries", () => {
  test("catalog state distinguishes platform setup from workspace connection", () => {
    expect(summarizeConnector({ id: "github", configured: false, connected: false })).toMatchObject({
      provider: "github",
      state: "unconfigured",
    });
    expect(summarizeConnector({ id: "github", configured: true, connected: false })).toMatchObject({
      provider: "github",
      state: "not_connected",
    });
    expect(summarizeConnector({ id: "github", configured: true, connected: true })).toEqual({
      provider: "github",
      display_name: "",
      archetype: "",
      state: "connected",
    });
  });

  test("provider status preserves reconnect and disconnected as successful states", () => {
    const entry = { id: "github", configured: true, connected: true };
    expect(summarizeStatus(entry, { status: "reconnect_required" })).toMatchObject({
      state: "reconnect_required",
      hint: expect.stringContaining("Reconnect github"),
    });
    expect(summarizeStatus(entry, { status: "not_connected" })).toMatchObject({
      state: "not_connected",
      hint: expect.stringContaining("Connect github"),
    });
  });

  test("newer connected status removes a stale catalog connect hint", () => {
    expect(summarizeStatus(
      { id: "github", configured: true, connected: false },
      { status: "connected" },
    )).toEqual({
      provider: "github",
      display_name: "",
      archetype: "",
      state: "connected",
      details: { status: "connected" },
    });
  });
});
