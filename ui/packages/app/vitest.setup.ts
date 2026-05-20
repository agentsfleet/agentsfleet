import { afterEach, vi } from "vitest";

// Reset to real timers after every test so a previous test's
// `vi.useFakeTimers()` cannot bleed into the next file's `waitFor`
// (which itself uses setTimeout internally and hangs if mocked).
afterEach(() => {
  vi.useRealTimers();
});
