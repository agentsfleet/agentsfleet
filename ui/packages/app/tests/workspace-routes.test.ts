import { describe, expect, it } from "vitest";
import {
  DEFAULT_WORKSPACE_SUBPATH,
  WORKSPACE_ROUTE_PREFIX,
  workspaceIdFromPath,
  workspacePath,
  workspaceSubpath,
  workspaceSwitchSubpath,
} from "../lib/workspace-routes";

describe("workspacePath", () => {
  it("builds the workspace root when no subpath is given", () => {
    expect(workspacePath("ws_1")).toBe("/w/ws_1");
  });

  it("appends a subpath", () => {
    expect(workspacePath("ws_1", "fleets")).toBe("/w/ws_1/fleets");
    expect(workspacePath("ws_1", "settings/models")).toBe("/w/ws_1/settings/models");
  });

  it("tolerates a leading slash on the subpath", () => {
    expect(workspacePath("ws_1", "/fleets")).toBe("/w/ws_1/fleets");
  });

  it("is prefixed by WORKSPACE_ROUTE_PREFIX", () => {
    expect(workspacePath("ws_1").startsWith(WORKSPACE_ROUTE_PREFIX)).toBe(true);
  });
});

describe("workspaceIdFromPath", () => {
  it("extracts the id under the /w segment", () => {
    expect(workspaceIdFromPath("/w/ws_1")).toBe("ws_1");
    expect(workspaceIdFromPath("/w/ws_1/fleets/abc")).toBe("ws_1");
  });

  it("returns null for tenant/platform routes (no /w segment)", () => {
    expect(workspaceIdFromPath("/settings/api-keys")).toBeNull();
    expect(workspaceIdFromPath("/admin/models")).toBeNull();
    expect(workspaceIdFromPath("/")).toBeNull();
  });
});

describe("workspaceSubpath", () => {
  it("returns the path after /w/<id> without a leading slash", () => {
    expect(workspaceSubpath("/w/ws_1/fleets/abc")).toBe("fleets/abc");
    expect(workspaceSubpath("/w/ws_1/settings/models")).toBe("settings/models");
  });

  it("returns empty string at the workspace root or off-segment", () => {
    expect(workspaceSubpath("/w/ws_1")).toBe("");
    expect(workspaceSubpath("/settings/api-keys")).toBe("");
  });

  it("round-trips with workspacePath to preserve the sub-page across a switch", () => {
    const sub = workspaceSubpath("/w/ws_a/fleets");
    expect(workspacePath("ws_b", sub)).toBe("/w/ws_b/fleets");
  });
});

describe("workspaceSwitchSubpath", () => {
  it("collapses a resource-detail path to its section (target ws won't own the id)", () => {
    expect(workspaceSwitchSubpath("fleets/fleet_123")).toBe("fleets");
    expect(workspaceSwitchSubpath("approvals/gate_9")).toBe("approvals");
  });

  it("preserves generic pages and maps an empty route to the workspace home", () => {
    expect(workspaceSwitchSubpath("fleets")).toBe("fleets");
    expect(workspaceSwitchSubpath("fleets/new")).toBe("fleets/new");
    expect(workspaceSwitchSubpath("integrations")).toBe("integrations");
    expect(workspaceSwitchSubpath("settings/models")).toBe("settings/models");
    expect(workspaceSwitchSubpath("")).toBe(DEFAULT_WORKSPACE_SUBPATH);
  });

  it("switching from a fleet detail lands on the target's fleets list, not a 404", () => {
    const target = workspacePath("ws_b", workspaceSwitchSubpath(workspaceSubpath("/w/ws_a/fleets/fleet_123")));
    expect(target).toBe("/w/ws_b/fleets");
  });
});
