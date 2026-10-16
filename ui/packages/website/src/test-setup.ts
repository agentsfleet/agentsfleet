import { vi } from "vitest";
import "@testing-library/jest-dom/vitest";

Object.defineProperty(window, "scrollTo", {
  value: () => {},
  writable: true,
});

Object.defineProperty(window, "matchMedia", { writable: true, value: vi.fn(() => ({
  matches: true, addEventListener: vi.fn(), removeEventListener: vi.fn(),
})) });
vi.stubGlobal("IntersectionObserver", class {
  observe() {} unobserve() {} disconnect() {}
});
