import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import FleetConfig from "./FleetConfig";
import { DELETE_MEMORY_TRAP_NOTICE } from "./console-copy";

vi.mock("../../actions", () => ({ deleteFleetAction: vi.fn() }));
vi.mock("next/navigation", () => ({ useRouter: () => ({ push: vi.fn(), refresh: vi.fn() }) }));

afterEach(() => cleanup());

function renderConfig() {
  return render(<FleetConfig workspaceId="ws_1" fleetId="agt_1" fleetName="platform-ops" />);
}

describe("FleetConfig", () => {
  it("test_config_card_no_stale_endpoint_copy", () => {
    renderConfig();
    // The M80-era claim that PATCH/pause/resume are unbuilt is gone (G7).
    expect(screen.queryByText(/backend adds/i)).toBeNull();
    expect(screen.queryByText(/:pause/)).toBeNull();
    expect(screen.queryByText(/:resume/)).toBeNull();
  });

  it("test_delete_confirm_states_memory_trap", async () => {
    const user = userEvent.setup({ delay: null });
    renderConfig();
    await user.click(screen.getByRole("button", { name: /delete fleet/i }));
    // The confirm states memory dies with the fleet, and that editing keeps it (G8).
    expect(screen.getByRole("alertdialog").textContent).toContain(DELETE_MEMORY_TRAP_NOTICE);
  });

  it("renders a standalone delete action for the fleet header", () => {
    const { container } = renderConfig();
    expect(screen.getByRole("button", { name: /delete fleet/i })).toBeTruthy();
    expect(container.querySelector("article")).toBeNull();
  });
});
