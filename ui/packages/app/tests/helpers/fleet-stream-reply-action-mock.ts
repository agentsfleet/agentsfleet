import { vi } from "vitest";

// The registry reads a settled reply through an installed reader. Each reply
// shard installs `fleetActionsMock().getFleetTurnAction` as that reader. This
// module imports nothing under test, so it never waits on the registry.
export const getFleetTurnActionMock = vi.fn();
export const failedAction = { enabled: false, calls: 0 };

export function fleetActionsMock() {
  return {
    getFleetTurnAction: (...args: unknown[]) => {
      if (failedAction.enabled) {
        failedAction.calls += 1;
        return Promise.reject(new Error("detail unavailable"));
      }
      return getFleetTurnActionMock(...args);
    },
  };
}

export function resetFleetTurnAction(): void {
  getFleetTurnActionMock.mockReset();
  getFleetTurnActionMock.mockResolvedValue({ ok: false });
  failedAction.enabled = false;
  failedAction.calls = 0;
}
