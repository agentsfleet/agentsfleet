import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { RunnerListResponse } from "@/lib/api/runners";

const refresh = vi.fn();
vi.mock("next/navigation", () => ({
  useRouter: () => ({ refresh }),
}));

// The enroll dialog is a dynamic island; the view's own duty is wiring its
// onCreated to a route refresh, which the stub lets the test drive directly.
vi.mock("@/components/domain/island-dynamic/AddRunnerDialogDynamic", async () => {
  const { Button } = await import("@agentsfleet/design-system");
  return {
    default: ({ onCreated }: { onCreated: () => void }) => (
      <Button onClick={onCreated}>Add runner</Button>
    ),
  };
});

vi.mock("../actions", () => ({
  listRunnersAction: vi.fn(),
  listRunnerLeasesAction: vi
    .fn()
    .mockResolvedValue({ ok: true, data: { items: [], total: 0, next_cursor: null } }),
}));

import RunnersView from "./RunnersView";

afterEach(() => cleanup());

const EMPTY: RunnerListResponse = { items: [], total: 0, next_cursor: null };

describe("RunnersView", () => {
  it("should render the page grammar over the wall — title, section, empty wall", () => {
    render(<RunnersView initial={EMPTY} />);
    expect(screen.getByText("Runners")).toBeTruthy();
    expect(screen.getByText("Manage runners")).toBeTruthy();
    expect(screen.getByLabelText("Runners")).toBeTruthy();
    expect(screen.getByText("No runners enrolled")).toBeTruthy();
  });

  it("should refresh the route when the enroll dialog reports a created runner", () => {
    render(<RunnersView initial={EMPTY} />);
    fireEvent.click(screen.getByRole("button", { name: "Add runner" }));
    expect(refresh).toHaveBeenCalledTimes(1);
  });
});
