import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { updateRunner, type RunnerEdit } from "./runners";
import { parseLabels } from "./runners-types";
import { response } from "@/tests/helpers/draft-fixtures";

const fetchMock = vi.fn<typeof fetch>();
const POLICY = {sandbox_tier: "container_nested", network_policy: "allow_all", registry_allowlist: [], worker_count: 2} as const;
const assigned_policy = {...POLICY, registry_allowlist: [], extra_binds: []};
const SENTENCE = "Runner labels must contain at most 32 labels of at most 64 bytes each";
beforeEach(() => {fetchMock.mockReset(); vi.stubGlobal("fetch", fetchMock);});
afterEach(() => vi.unstubAllGlobals());
describe("runner edit transport", () => {
  it.each<RunnerEdit>([{labels: []}, {labels: ["gpu", "gpu", "us-east"]}, {assigned_policy}, {assigned_policy, labels: ["gpu"]}])("sends one PATCH for edit %j and returns all nullable reply fields", async (edit) => {
    const reply = {id: "runner/one", status: "active", assigned_policy: edit.assigned_policy ?? null, labels: edit.labels ? [...new Set(edit.labels)] : null, selftest_requested_at: null};
    fetchMock.mockResolvedValueOnce(response(reply));
    expect(await updateRunner("session", "runner/one", edit)).toEqual(reply);
    expect(fetchMock).toHaveBeenCalledOnce();
    const [url, init] = fetchMock.mock.calls[0] ?? [];
    expect(typeof url === "string" ? url : url instanceof URL ? url.href : url?.url).toContain("/v1/fleets/runners/runner%2Fone");
    expect(init?.method).toBe("PATCH");
    expect(init?.body).toBe(JSON.stringify(edit));
  });
  it.each([400, 401, 403])("keeps the server sentence and status %i on refusal", async (status) => {
    fetchMock.mockResolvedValueOnce(response({detail: SENTENCE, error_code: "UZ-RUN-001"}, status));
    await expect(updateRunner("session", "runner/one", {labels: []})).rejects.toMatchObject({name: "ApiError", message: SENTENCE, status, code: "UZ-RUN-001"});
    expect(fetchMock).toHaveBeenCalledOnce();
  });
});
describe("label bounds shared by enrolment and editing", () => {
  it("accepts 32 labels and 64 ASCII bytes and preserves first-given order", () => {
    const labels = Array.from({length: 32}, (_, n) => `label-${n}`);
    expect(parseLabels(labels.join(","))).toEqual({labels, error: null});
    expect(parseLabels("x".repeat(64))).toEqual({labels: ["x".repeat(64)], error: null});
    expect(parseLabels("gpu,us-east,gpu")).toEqual({labels: ["gpu", "us-east"], error: null});
  });
  it.each(["x".repeat(65), Array.from({length: 33}, (_, n) => `label-${n}`).join(",")])("rejects bounds overflow %s", (raw) => {
    expect(parseLabels(raw)).toEqual({labels: [], error: expect.stringMatching(/must be|at most 32/i)});
  });
});
