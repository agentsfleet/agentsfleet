import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { RunnerListItem } from "@/lib/api/runners";
import { RunnerActionConfirm } from "./RunnerDialogs";
import { ACTION_CONFIG, DELETE_ACTION_CONFIG, actionsFor, canDelete } from "./RunnerListCells";

afterEach(() => cleanup());

const RUNNER: RunnerListItem = {
  id: "r-dialog-1",
  host_id: "host-1",
  sandbox_tier: "landlock_full",
  admin_state: "active",
  liveness: "online",
  labels: [],
  last_seen_at: 0,
  created_at: 0,
};

describe("runner admin actions", () => {
  it("test_runner_admin_actions_unchanged", () => {
    // The exact confirm copy the retired table applied, unchanged.
    expect(ACTION_CONFIG.cordon.title).toBe("Cordon this runner?");
    expect(ACTION_CONFIG.drain.title).toBe("Drain this runner?");
    expect(ACTION_CONFIG.revoke.title).toBe("Revoke this runner?");
    expect(ACTION_CONFIG.revoke.intent).toBe("destructive");
    expect(DELETE_ACTION_CONFIG.title).toBe("Delete this runner?");

    // The same eligibility rules: cordon only while active, drain until
    // draining, revoke until revoked, delete only once revoked.
    expect(actionsFor("active")).toEqual(["cordon", "drain", "revoke"]);
    expect(actionsFor("cordoned")).toEqual(["drain", "revoke"]);
    expect(actionsFor("draining")).toEqual(["revoke"]);
    expect(actionsFor("drained")).toEqual(["revoke"]);
    expect(actionsFor("revoked")).toEqual([]);
    expect(canDelete("revoked")).toBe(true);
    expect(canDelete("active")).toBe(false);
  });

  it("renders the target's copy and confirms with its own handler", () => {
    const onConfirm = vi.fn();
    render(
      <RunnerActionConfirm
        target={{ runner: RUNNER, action: "cordon", ...ACTION_CONFIG.cordon }}
        error={null}
        onOpenChange={vi.fn()}
        onConfirm={onConfirm}
      />,
    );
    expect(screen.getByText("Cordon this runner?")).toBeTruthy();
    expect(screen.getByText(ACTION_CONFIG.cordon.description)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Cordon" }));
    expect(onConfirm).toHaveBeenCalledWith(
      expect.objectContaining({ action: "cordon", runner: RUNNER }),
    );
  });
});
