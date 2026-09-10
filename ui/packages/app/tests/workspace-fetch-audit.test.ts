import { afterEach, describe, expect, it, vi } from "vitest";

import {
  AUDITED_PATH,
  beginWorkspaceFetchOutcome,
  readWorkspaceFetchTimings,
  FLEET_RUNNERS_PATH,
  TENANT_PROVIDER_PATH,
  WORKSPACE_LIST_PATH,
  isWorkspaceFetchAuditEnabled,
  readWorkspaceFetchAudit,
  recordWorkspaceFetchForAcceptance,
  resetWorkspaceFetchAudit,
} from "../lib/acceptance/workspace-fetch-audit";

import {
  GET as getWorkspaceFetchAudit,
  POST as resetWorkspaceFetchAuditRoute,
} from "../app/acceptance-audit/workspace-fetches/route";

const AUDIT_TOKEN = "test-acceptance-token";
const AUTHORIZED_REQUEST = new Request("http://localhost/acceptance-audit/workspace-fetches", {
  headers: { "x-acceptance-token": AUDIT_TOKEN },
});
const UNAUTHORIZED_REQUEST = new Request("http://localhost/acceptance-audit/workspace-fetches");

afterEach(() => {
  vi.unstubAllEnvs();
  resetWorkspaceFetchAudit();
});

describe("workspace fetch acceptance audit", () => {
  it("stays inactive unless explicitly enabled", () => {
    expect(isWorkspaceFetchAuditEnabled()).toBe(false);

    recordWorkspaceFetchForAcceptance(WORKSPACE_LIST_PATH);
    recordWorkspaceFetchForAcceptance("/v1/other");

    expect(readWorkspaceFetchAudit()).toEqual({ total: 0, byPath: {} });
  });

  it("counts only workspace-list fetches and returns snapshots", () => {
    vi.stubEnv("AGENTSFLEET_E2E_AUDIT", "1");

    recordWorkspaceFetchForAcceptance(WORKSPACE_LIST_PATH);
    recordWorkspaceFetchForAcceptance(WORKSPACE_LIST_PATH);
    recordWorkspaceFetchForAcceptance("/v1/other");

    expect(readWorkspaceFetchAudit()).toEqual({
      total: 2,
      byPath: { [WORKSPACE_LIST_PATH]: 2 },
    });
    expect(resetWorkspaceFetchAudit()).toEqual({ total: 0, byPath: {} });
  });

  it("strips query strings and counts thread and detail reads under template keys", () => {
    vi.stubEnv("AGENTSFLEET_E2E_AUDIT", "1");

    // Real calls carry queries — the exact-match era silently counted zero.
    recordWorkspaceFetchForAcceptance(`${WORKSPACE_LIST_PATH}?limit=100`);
    recordWorkspaceFetchForAcceptance(
      "/v1/workspaces/ws_1/fleets/zom_1/messages?limit=20",
    );
    recordWorkspaceFetchForAcceptance(
      "/v1/workspaces/ws_1/fleets/zom_1/events/1700000000000-0",
    );
    // The events LIST and the live tail are deliberately unaudited.
    recordWorkspaceFetchForAcceptance("/v1/workspaces/ws_1/fleets/zom_1/events?limit=25");
    recordWorkspaceFetchForAcceptance("/v1/workspaces/ws_1/fleets/zom_1/events/stream");

    expect(readWorkspaceFetchAudit()).toEqual({
      total: 3,
      byPath: {
        [AUDITED_PATH.workspaceList]: 1,
        [AUDITED_PATH.fleetMessages]: 1,
        [AUDITED_PATH.fleetEventDetail]: 1,
      },
    });
  });

  it("counts the Secrets pair and the Runners list under template keys", () => {
    vi.stubEnv("AGENTSFLEET_E2E_AUDIT", "1");

    // The two reads the Secrets page awaits together.
    recordWorkspaceFetchForAcceptance("/v1/workspaces/ws_1/secrets");
    recordWorkspaceFetchForAcceptance(TENANT_PROVIDER_PATH);
    // The Runners list, with the keyset query a real call carries.
    recordWorkspaceFetchForAcceptance(`${FLEET_RUNNERS_PATH}?limit=50`);
    // Neighbours that must NOT be mistaken for the reads above: a single
    // secret by name, and one runner by id. Both are writes-adjacent detail
    // routes, and counting them would inflate a page's read inventory.
    recordWorkspaceFetchForAcceptance("/v1/workspaces/ws_1/secrets/OPENAI_API_KEY");
    recordWorkspaceFetchForAcceptance(`${FLEET_RUNNERS_PATH}/rnr_1`);

    expect(readWorkspaceFetchAudit()).toEqual({
      total: 3,
      byPath: {
        [AUDITED_PATH.workspaceSecrets]: 1,
        [AUDITED_PATH.tenantProvider]: 1,
        [AUDITED_PATH.fleetRunners]: 1,
      },
    });
  });

  it("records what a settled request cost, per template", () => {
    vi.stubEnv("AGENTSFLEET_E2E_AUDIT", "1");

    const outcome = beginWorkspaceFetchOutcome(`${FLEET_RUNNERS_PATH}?limit=50`);
    // Three attempts, as a retry ladder that had to climb would report.
    const track = outcome.trackAttempts();
    for (const attempt of [1, 2, 3]) track({ attempt });
    outcome.settle();

    const timings = readWorkspaceFetchTimings();
    const runners = timings[AUDITED_PATH.fleetRunners];
    expect(runners?.attempts, "the attempts the ladder took").toEqual([3]);
    expect(runners?.durationsMs.length, "one duration per settled request").toBe(1);
    expect(runners?.durationsMs[0]).toBeGreaterThanOrEqual(0);
  });

  it("wraps a caller's attempt callback instead of replacing it", () => {
    vi.stubEnv("AGENTSFLEET_E2E_AUDIT", "1");

    // Instrumenting the transport must not silence a caller that is already
    // watching its own attempts — client.retry.test.ts asserts exactly that.
    const seen: number[] = [];
    const outcome = beginWorkspaceFetchOutcome(FLEET_RUNNERS_PATH);
    const track = outcome.trackAttempts((info: { attempt: number }) => seen.push(info.attempt));
    track({ attempt: 1 });
    track({ attempt: 2 });
    outcome.settle();

    expect(seen, "the caller's callback still fires for every attempt").toEqual([1, 2]);
    expect(readWorkspaceFetchTimings()[AUDITED_PATH.fleetRunners]?.attempts).toEqual([2]);
  });

  it("records nothing for an unaudited path or while disabled", () => {
    const disabled = beginWorkspaceFetchOutcome(FLEET_RUNNERS_PATH);
    disabled.settle();
    expect(readWorkspaceFetchTimings(), "the env gate is off").toEqual({});

    vi.stubEnv("AGENTSFLEET_E2E_AUDIT", "1");
    const unaudited = beginWorkspaceFetchOutcome("/v1/something/not/audited");
    unaudited.settle();
    expect(readWorkspaceFetchTimings(), "an unaudited template stays absent").toEqual({});
  });

  it("guards the route while disabled", async () => {
    const getResponse = getWorkspaceFetchAudit(UNAUTHORIZED_REQUEST);
    expect(getResponse.status).toBe(404);
    await expect(getResponse.json()).resolves.toEqual({
      error: "acceptance_audit_disabled",
    });

    const postResponse = resetWorkspaceFetchAuditRoute(UNAUTHORIZED_REQUEST);
    expect(postResponse.status).toBe(404);
    await expect(postResponse.json()).resolves.toEqual({
      error: "acceptance_audit_disabled",
    });
  });

  it("requires the acceptance token while enabled", async () => {
    vi.stubEnv("AGENTSFLEET_E2E_AUDIT", "1");
    vi.stubEnv("AGENTSFLEET_E2E_AUDIT_TOKEN", AUDIT_TOKEN);

    const getResponse = getWorkspaceFetchAudit(UNAUTHORIZED_REQUEST);
    expect(getResponse.status).toBe(401);
    await expect(getResponse.json()).resolves.toEqual({
      error: "acceptance_audit_unauthorized",
    });

    const postResponse = resetWorkspaceFetchAuditRoute(UNAUTHORIZED_REQUEST);
    expect(postResponse.status).toBe(401);
    await expect(postResponse.json()).resolves.toEqual({
      error: "acceptance_audit_unauthorized",
    });
  });

  it("serves and resets the audit snapshot while enabled", async () => {
    vi.stubEnv("AGENTSFLEET_E2E_AUDIT", "1");
    vi.stubEnv("AGENTSFLEET_E2E_AUDIT_TOKEN", AUDIT_TOKEN);
    recordWorkspaceFetchForAcceptance(WORKSPACE_LIST_PATH);

    const getResponse = getWorkspaceFetchAudit(AUTHORIZED_REQUEST);
    expect(getResponse.status).toBe(200);
    await expect(getResponse.json()).resolves.toEqual({
      total: 1,
      byPath: { [WORKSPACE_LIST_PATH]: 1 },
      // Counts come from the ask, timings from the settle; a request that was
      // recorded but never settled has a count and no timing.
      timingsByPath: {},
    });

    const postResponse = resetWorkspaceFetchAuditRoute(AUTHORIZED_REQUEST);
    expect(postResponse.status).toBe(200);
    await expect(postResponse.json()).resolves.toEqual({ total: 0, byPath: {} });
  });
});
