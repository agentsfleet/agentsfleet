import { act, type RenderResult } from "@testing-library/react";
import { vi } from "vitest";

export const LOG_PX = 1_000;
export const RESERVE = "[data-aui-top-anchor-reserve]";
const FRAME_PX = 400;

export type Frame = ReturnType<typeof frameOf>;

/**
 * Stands in for a browser's layout: animation frames, the resize observers of
 * each box a step resized, and the viewport's geometry. Install it before the
 * render, since the library observes the viewport as it mounts.
 */
export function stubBrowserLayout() {
  const observers = new Set<{ targets: Set<Element>; notify: () => void }>();
  const frames = new Map<number, FrameRequestCallback>();
  let nextFrame = 0;
  vi.stubGlobal("ResizeObserver", class {
    readonly #targets = new Set<Element>();
    constructor(callback: ResizeObserverCallback) {
      observers.add({ targets: this.#targets, notify: () => callback([], this) });
    }
    observe(target: Element): void { this.#targets.add(target); }
    unobserve(target: Element): void { this.#targets.delete(target); }
    disconnect(): void { this.#targets.clear(); }
  });
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
    nextFrame += 1;
    frames.set(nextFrame, callback);
    return nextFrame;
  });
  vi.stubGlobal("cancelAnimationFrame", (id: number) => frames.delete(id));
  return {
    /** One rendering step: queued animation frames, then the observers of every resized box. */
    layout(...resized: Element[]): void {
      act(() => {
        const due = [...frames.values()];
        frames.clear();
        for (const callback of due) callback(0);
        for (const observer of [...observers]) {
          if (resized.some(target => observer.targets.has(target))) observer.notify();
        }
      });
    },
    frameOf,
  };
}

function frameOf(view: RenderResult) {
  const viewport = view.getByTestId("fleet-thread-root").querySelector<HTMLElement>('[role="presentation"]');
  // The thread's own column around the log, inside the viewport's content.
  const log = view.getByRole("log").closest<HTMLElement>('[role="presentation"] > div > div');
  const content = viewport?.firstElementChild;
  if (!viewport || !log || !content) throw new Error("the thread viewport is not mounted");
  const size = { log: LOG_PX, frame: FRAME_PX };
  let scrollTop = 0;
  const reserve = () => Number.parseFloat(viewport.querySelector<HTMLElement>(RESERVE)?.style.height ?? "") || 0;
  Object.defineProperties(viewport, {
    // The library sizes its reserve from scrollHeight less the reserve's
    // offsetHeight, which happy-dom reports as 0: this is the log alone.
    scrollHeight: { configurable: true, get: () => size.log },
    clientHeight: { configurable: true, get: () => size.frame },
    scrollTop: { configurable: true, get: () => scrollTop, set: (top: number) => { scrollTop = top; } },
  });
  const scrollTo = vi.fn((options?: ScrollToOptions | number) => {
    if (typeof options !== "object") return;
    scrollTop = Math.max(0, Math.min(options.top ?? scrollTop, size.log + reserve() - size.frame));
  });
  viewport.scrollTo = scrollTo;
  return { viewport, log, content, size, scrollTo, footer: view.getByTestId("fleet-chat-footer"),
    composer: view.getByRole("textbox", { name: "Message your fleet…" }) };
}
