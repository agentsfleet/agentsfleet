import { vi } from "vitest";

// The registry reads a settled reply through the Server Action. Each reply
// shard's hoisted `vi.mock` delegates here. This module imports nothing under
// test, so the factory never waits on the registry it is being loaded for.
export const getFleetEventActionMock = vi.fn();
export const failedAction = { enabled: false, calls: 0 };

export function fleetActionsMock() {
  return {
    getFleetEventAction: (...args: unknown[]) => {
      if (failedAction.enabled) {
        failedAction.calls += 1;
        return Promise.reject(new Error("detail unavailable"));
      }
      return getFleetEventActionMock(...args);
    },
  };
}

export function resetFleetEventAction(): void {
  getFleetEventActionMock.mockReset();
  getFleetEventActionMock.mockResolvedValue({ ok: false });
  failedAction.enabled = false;
  failedAction.calls = 0;
}
