import { beforeEach, expect, it, vi } from "vitest";
import { listRunnerLeases } from "./runners";

const { requestMock } = vi.hoisted(() => ({requestMock: vi.fn()}));
vi.mock("./client", () => ({request: requestMock}));
beforeEach(() => requestMock.mockReset());

const ROW = {id: "lease", fleet_id: "fleet", fleet_name: null, destination: "fleet_deleted",
  workspace_id: "workspace", event_id: "event", event_type: null, actor: null, outcome: "unknown",
  failure_label: null, failure_detail: null, kind: "fresh", fencing_token: 1, provider: "fireworks", model: "glm",
  posture: "metered", metered_input_tokens: 0, metered_cached_tokens: 0, metered_output_tokens: 0,
  wall_ms: null, lease_expires_at: "2026-10-19T01:00:00Z", created_at: "2026-10-19T00:00:00Z"};
const PAGE = {items: [ROW], total: 1, next_cursor: "retained-cursor"};

it("keeps a retained lease after its fleet and event disappear", async () => {
  requestMock.mockResolvedValue(PAGE);
  expect(await listRunnerLeases("token", "runner")).toEqual(PAGE);
  expect(requestMock).toHaveBeenCalledExactlyOnceWith("/v1/fleets/runners/runner/leases", {method: "GET"}, "token");
});

it.each([
  {}, {...PAGE, items: null}, {...PAGE, next_cursor: {}},
  {...PAGE, items: [{...ROW, destination: undefined}]},
  {...PAGE, items: [{...ROW, destination: "yes"}]},
  {...PAGE, items: [{...ROW, event_type: 3}]},
  {...PAGE, items: [{...ROW, actor: {name: "system"}}]},
  {...PAGE, items: [{...ROW, created_at: 3}]},
  {...PAGE, items: [{...ROW, metered_input_tokens: "0"}]},
])("refuses malformed retained history before the page can render: %j", async (body) => {
  requestMock.mockResolvedValue(body);
  await expect(listRunnerLeases("token", "runner")).rejects.toThrow();
});
