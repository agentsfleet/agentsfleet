/**
 * The header adds the canvas's reserved scrollbar width to land on the canvas's
 * content edge. That width is not a constant: it is the platform's, and on any
 * platform whose scrollbars overlay it is zero. These tests pin the measurement
 * and the two states it has to tell apart.
 */
import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, waitFor } from "@testing-library/react";
import {
  CanvasScrollbarProbe,
  reservedGutterWidth,
} from "../components/layout/CanvasScrollbarProbe";

const SCROLLBAR_VAR = "--app-scrollbar";
const CANVAS_CLASS = "app-dashboard-canvas";

/** happy-dom reports 0 for both, so the widths are installed explicitly. */
function canvasWith(offsetWidth: number, clientWidth: number): HTMLElement {
  const canvas = document.createElement("main");
  canvas.className = CANVAS_CLASS;
  Object.defineProperty(canvas, "offsetWidth", { value: offsetWidth, configurable: true });
  Object.defineProperty(canvas, "clientWidth", { value: clientWidth, configurable: true });
  document.body.appendChild(canvas);
  return canvas;
}

afterEach(() => {
  cleanup();
  document.body.innerHTML = "";
  document.documentElement.style.removeProperty(SCROLLBAR_VAR);
  vi.unstubAllGlobals();
});

describe("reservedGutterWidth", () => {
  it("halves the pair of gutters a both-edges reservation takes", () => {
    expect(reservedGutterWidth(canvasWith(1679, 1667))).toBe(6);
  });

  it("reports zero where the scrollbar overlays and reserves nothing", () => {
    // The whole of iOS, and any desktop route whose canvas does not scroll.
    expect(reservedGutterWidth(canvasWith(1679, 1679))).toBe(0);
  });

  it("refuses a negative width rather than pulling the header outward", () => {
    expect(reservedGutterWidth(canvasWith(1667, 1679))).toBe(0);
  });
});

describe("CanvasScrollbarProbe", () => {
  it("publishes the measured width to the token the header reads", async () => {
    canvasWith(1679, 1667);
    const observe = vi.fn();
    vi.stubGlobal(
      "ResizeObserver",
      class {
        observe = observe;
        disconnect = vi.fn();
      },
    );

    render(React.createElement(CanvasScrollbarProbe));

    await waitFor(() =>
      expect(document.documentElement.style.getPropertyValue(SCROLLBAR_VAR)).toBe("6px"),
    );
    // Observed, because the reservation comes and goes with the content.
    expect(observe).toHaveBeenCalledTimes(1);
  });

  it("writes 0px rather than leaving the stylesheet's desktop default standing", async () => {
    canvasWith(1679, 1679);
    vi.stubGlobal(
      "ResizeObserver",
      class {
        observe = vi.fn();
        disconnect = vi.fn();
      },
    );

    render(React.createElement(CanvasScrollbarProbe));

    await waitFor(() =>
      expect(document.documentElement.style.getPropertyValue(SCROLLBAR_VAR)).toBe("0px"),
    );
  });

  it("does nothing at all when the shell has no canvas", async () => {
    const disconnect = vi.fn();
    vi.stubGlobal(
      "ResizeObserver",
      class {
        observe = vi.fn();
        disconnect = disconnect;
      },
    );

    const { unmount } = render(React.createElement(CanvasScrollbarProbe));
    unmount();

    expect(document.documentElement.style.getPropertyValue(SCROLLBAR_VAR)).toBe("");
    expect(disconnect).not.toHaveBeenCalled();
  });

  it("stops observing when the shell unmounts", async () => {
    canvasWith(1679, 1667);
    const disconnect = vi.fn();
    vi.stubGlobal(
      "ResizeObserver",
      class {
        observe = vi.fn();
        disconnect = disconnect;
      },
    );

    const { unmount } = render(React.createElement(CanvasScrollbarProbe));
    await waitFor(() =>
      expect(document.documentElement.style.getPropertyValue(SCROLLBAR_VAR)).toBe("6px"),
    );
    unmount();

    expect(disconnect).toHaveBeenCalledTimes(1);
  });
});
