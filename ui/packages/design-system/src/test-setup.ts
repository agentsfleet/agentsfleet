import "@testing-library/jest-dom/vitest";
import { afterEach, vi } from "vitest";

// Reset to real timers after every test so a previous test's
// `vi.useFakeTimers()` cannot bleed into the next file's `waitFor`
// (which itself uses setTimeout internally and hangs if mocked).
afterEach(() => {
  vi.useRealTimers();
});

// jsdom has no scrolling implementation; model the browser method for table tests.
HTMLElement.prototype.scrollTo = function (options?: ScrollToOptions | number, y?: number) {
  if (typeof options === "object") {
    this.scrollTop = options.top ?? this.scrollTop;
    this.scrollLeft = options.left ?? this.scrollLeft;
  } else {
    this.scrollLeft = options ?? this.scrollLeft;
    this.scrollTop = y ?? this.scrollTop;
  }
};
