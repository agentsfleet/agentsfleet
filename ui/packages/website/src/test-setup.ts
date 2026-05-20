import "@testing-library/jest-dom/vitest";
import { afterEach, vi } from "vitest";

Object.defineProperty(window, "scrollTo", {
  value: () => {},
  writable: true,
});

// Reset to real timers after every test so a previous test's
// `vi.useFakeTimers()` cannot bleed into the next file's `waitFor`.
afterEach(() => {
  vi.useRealTimers();
});
