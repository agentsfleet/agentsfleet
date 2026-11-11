import { afterEach, describe, expect, it, vi } from "vitest";
import { findRegisteredModelAction, readModelActivationAction } from "@/app/(dashboard)/w/[workspaceId]/settings/models/actions";

vi.mock("@clerk/nextjs/server", () => ({auth: async () => ({getToken: async () => "unit-test-token"})}));

const MAIN = "0199a6f0-1c2d-7e3f-8a4b-5c6d7e8f9a0b";
const STAGING = "0199a6f0-1c2d-7e3f-8a4b-5c6d7e8f9a0c";
const request = {workspace_id: STAGING, model_id: "model", secret_ref: "shared-name"};
const entry = (workspace: string, id: string) => ({
  id, workspace_id: workspace, model_id: request.model_id, secret_ref: request.secret_ref,
  kind: "provider_key", provider: "openai", has_key: true, active: false, created_at: "2026-11-11T10:00:00Z",
});
const page = (models: ReturnType<typeof entry>[], cursor: string | null) => Response.json({
  models, next_cursor: cursor, total: null, platform_default_available: false,
});
afterEach(() => vi.unstubAllGlobals());

describe("model source reconciliation through server actions and HTTP clients", () => {
  it("continues past a same-named Main credential to find the submitted Staging entry", async () => {
    const staging = entry(STAGING, "staging-entry");
    const fetch = vi.fn()
      .mockResolvedValueOnce(page([entry(MAIN, "main-entry")], "next-page"))
      .mockResolvedValueOnce(page([staging], null));
    vi.stubGlobal("fetch", fetch);
    expect(await findRegisteredModelAction(request)).toEqual({ok: true, data: staging});
    expect(fetch).toHaveBeenCalledTimes(2);
    expect(String(fetch.mock.calls[1]?.[0])).toContain("starting_after=next-page");
  });

  it("does not confirm registration from another workspace's matching model and key name", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(page([entry(MAIN, "main-entry")], null)));
    expect(await findRegisteredModelAction(request)).toEqual({ok: true, data: null});
  });

  it.each([
    [MAIN, false], [STAGING, true], [undefined, false],
  ])("confirms activation only for the stored workspace (%s)", async (workspace, expected) => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(Response.json({
      mode: "self_managed", model: request.model_id, secret_ref: request.secret_ref,
      workspace_id: workspace, provider: "openai", context_cap_tokens: 64_000,
      platform_default_available: false,
    })));
    expect(await readModelActivationAction(request)).toEqual({ok: true, data: expected});
  });
});
