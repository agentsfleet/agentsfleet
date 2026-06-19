import { afterEach, describe, expect, it, vi } from "vitest";

import {
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
    });

    const postResponse = resetWorkspaceFetchAuditRoute(AUTHORIZED_REQUEST);
    expect(postResponse.status).toBe(200);
    await expect(postResponse.json()).resolves.toEqual({ total: 0, byPath: {} });
  });
});
