import { cleanup } from "@testing-library/react";
import { afterEach, vi } from "vitest";

// jsdom supplies navigator. React 19.3.0 and Vitest 5.0.3 are pinned by the
// source workspace; clipboard is replaced only at the browser boundary.
afterEach(() => {
  cleanup();
  vi.useRealTimers();
});
