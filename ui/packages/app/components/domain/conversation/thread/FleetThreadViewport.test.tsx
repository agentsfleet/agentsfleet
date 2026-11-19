import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, type RenderResult } from "@testing-library/react";
import { useRef, type ReactNode } from "react";
import type { ThreadMessageLike } from "@assistant-ui/react";
import {
  AssistantRuntimeProvider,
  useExternalStoreRuntime,
} from "@assistant-ui/react";
import { FleetConversationLayout } from "../../shared/summary/FleetWorkSidebar";
import { FleetThreadViewport } from "./FleetThreadViewport";
import { useOpenAtLatest } from "../../shared/chat/useOpenAtLatest";
import { CONNECTION_STATUS, type ConnectionStatus } from "../stream/useFleetEventStream";

const NO_MESSAGES: ThreadMessageLike[] = [];
// An operator's turn with its reply still running, after an earlier one: the
// library never anchors the thread's first message.
const RUNNING_TURN: ThreadMessageLike[] = [
  { id: "turn-0", role: "user", content: "check the build" },
  { id: "turn-0:reply", role: "assistant", content: "Green." },
  { id: "turn-1", role: "user", content: "deploy the canary" },
  { id: "turn-1:reply", role: "assistant", content: "" },
];
const TOP_ANCHOR_USER = "[data-aui-top-anchor-user]";
const RESERVE = "[data-aui-top-anchor-reserve]";
const SETTLED_TURN = RUNNING_TURN.slice(0, 2);
const TEAMMATE_REPLY = (text: string): ThreadMessageLike[] => [...SETTLED_TURN,
  { id: "turn-1", role: "user", content: "summarise the rollout" },
  { id: "turn-1:reply", role: "assistant", content: text, status: { type: "running" } }];
const FIRST_SEND = (text: string): ThreadMessageLike[] => [
  { id: "turn-0", role: "user", content: "check the build" },
  { id: "turn-0:reply", role: "assistant", content: text, status: { type: "running" } }];
// What a browser would report for the thread; happy-dom lays nothing out.
const LOG_PX = 1_000;
const FRAME_PX = 400;
const SENT_TOP_PX = 820;
const SENT_HEIGHT_PX = 40;

afterEach(() => cleanup());

function Runtime({ children }: { children: ReactNode }) {
  const runtime = useExternalStoreRuntime<ThreadMessageLike>({
    messages: NO_MESSAGES, isRunning: false, convertMessage: (message) => message, onNew: async () => {},
  });
  return <AssistantRuntimeProvider runtime={runtime}>{children}</AssistantRuntimeProvider>;
}

/** Refs that never attach, as before an element mounts. */
function Detached() {
  const none = useRef<HTMLDivElement>(null);
  useOpenAtLatest(none, none);
  return null;
}

function View({ eventsCount = 0, messages = NO_MESSAGES, isRunning = false, connectionStatus = CONNECTION_STATUS.LIVE, setup, setupTurnId, work }: {
  eventsCount?: number;
  messages?: ThreadMessageLike[];
  isRunning?: boolean;
  connectionStatus?: ConnectionStatus;
  setup?: ReactNode;
  work?: ReactNode;
  setupTurnId?: string;
}) {
  const runtime = useExternalStoreRuntime<ThreadMessageLike>({
    messages,
    isRunning,
    convertMessage: (message) => message,
    onNew: async () => {},
  });
  return (
    <AssistantRuntimeProvider runtime={runtime}>
      <FleetConversationLayout work={work}><FleetThreadViewport
        setup={setup} setupTurnId={setupTurnId}
        eventsCount={eventsCount}
        connectionStatus={connectionStatus}
        onRetry={() => {}}
        pending={[]}
        onResend={() => {}}
        onDismiss={() => {}}
        onRestored={() => {}}
        onDraft={() => {}}
      /></FleetConversationLayout>
    </AssistantRuntimeProvider>
  );
}

describe("FleetThreadViewport scroll", () => {
  // jsdom does no layout, so the held view itself is proven in
  // fleet-thread-anchor.spec.ts. This pins the mode that holds it: while a
  // reply runs, the operator's newest row is the library's top anchor.
  it("anchors the operator's newest row while its reply runs", () => {
    const view = render(<View messages={RUNNING_TURN} />);
    expect(view.container.querySelector(TOP_ANCHOR_USER)).toBeNull();

    view.rerender(<View messages={RUNNING_TURN} isRunning />);
    const anchor = view.container.querySelector(TOP_ANCHOR_USER);
    expect(anchor?.textContent).toContain("deploy the canary");
  });

  // The chat scrolls like Claude: it opens at its latest turn, a sent turn
  // rises to the top, and nothing after that moves the view but the reader.
  describe("over a laid-out frame", () => {
    afterEach(() => vi.unstubAllGlobals());

    it("opens at the latest turn and lands again while settled history lays out", () => {
      const browser = stubBrowserLayout();
      const frame = browser.frameOf(render(<View messages={SETTLED_TURN} eventsCount={1} />));
      browser.layout(frame.log);
      expect(frame.scrollTo).toHaveBeenLastCalledWith({ top: LOG_PX, behavior: "instant" });
      expect(frame.viewport.scrollTop).toBe(600);
      // A settled reply laid out taller than its placeholder.
      frame.size.log = 1_300;
      browser.layout(frame.log);
      expect(frame.scrollTo).toHaveBeenLastCalledWith({ top: 1_300, behavior: "instant" });
      expect(frame.viewport.scrollTop).toBe(900);
    });

    it.each([
      ["scrolls up", (frame: Frame) => { frame.viewport.scrollTop = 200; fireEvent.scroll(frame.viewport); }, 200],
      ["types a draft", (frame: Frame) => { fireEvent.input(frame.composer, { target: { value: "Hold the rollout" } }); }, 600],
    ] as const)("stops landing once the reader %s", (_, act, held) => {
      const browser = stubBrowserLayout();
      const frame = browser.frameOf(render(<View messages={SETTLED_TURN} eventsCount={1} />));
      browser.layout(frame.log);
      act(frame);
      frame.size.log = 1_300;
      browser.layout(frame.log);
      expect(frame.scrollTo).toHaveBeenCalledTimes(1);
      expect(frame.viewport.scrollTop).toBe(held);
    });

    it("keeps landing when the reader scrolls but stays at the latest turn", () => {
      const browser = stubBrowserLayout();
      const frame = browser.frameOf(render(<View messages={SETTLED_TURN} eventsCount={1} />));
      browser.layout(frame.log);
      fireEvent.scroll(frame.viewport);
      frame.size.log = 1_300;
      browser.layout(frame.log);
      expect(frame.scrollTo).toHaveBeenCalledTimes(2);
      expect(frame.viewport.scrollTop).toBe(900);
    });

    it("does nothing until its viewport and log exist", () => {
      const observed = vi.fn();
      vi.stubGlobal("ResizeObserver", class { observe = observed; disconnect(): void {} });
      render(<Runtime><Detached /></Runtime>);
      expect(observed).not.toHaveBeenCalled();
    });

    it("opens at history that loads after the chat mounts", () => {
      const browser = stubBrowserLayout();
      const view = render(<View />);
      const frame = browser.frameOf(view);
      browser.layout(frame.log);
      expect(frame.scrollTo).not.toHaveBeenCalled();
      view.rerender(<View messages={SETTLED_TURN} eventsCount={1} />);
      browser.layout(frame.log, frame.content);
      expect(frame.scrollTo).toHaveBeenLastCalledWith({ top: LOG_PX, behavior: "instant" });
      expect(frame.viewport.scrollTop).toBe(600);
    });

    it("leaves an empty chat where it is while its first turn's reply grows", () => {
      const browser = stubBrowserLayout();
      const view = render(<View />);
      const frame = browser.frameOf(view);
      browser.layout(frame.log);
      for (const [text, log] of [["", 300], ["Green, and the canary is healthy.", 700]] as const) {
        view.rerender(<View messages={FIRST_SEND(text)} eventsCount={1} isRunning />);
        frame.size.log = log;
        browser.layout(frame.log, frame.content);
      }
      expect(frame.scrollTo).not.toHaveBeenCalled();
      expect(frame.viewport.scrollTop).toBe(0);
    });

    it("lifts a sent turn to the top over a reserve, then holds it while the reply streams", () => {
      const browser = stubBrowserLayout();
      const view = render(<View messages={SETTLED_TURN} eventsCount={1} />);
      const frame = browser.frameOf(view);
      browser.layout(frame.log);
      view.rerender(<View messages={RUNNING_TURN} eventsCount={2} isRunning />);
      const sent = view.container.querySelector<HTMLElement>(TOP_ANCHOR_USER);
      if (sent === null) throw new Error("the sent turn is not the top anchor");
      Object.defineProperties(sent, { offsetTop: { value: SENT_TOP_PX }, offsetHeight: { value: SENT_HEIGHT_PX } });
      browser.layout(frame.log, frame.content);
      expect(frame.scrollTo).toHaveBeenLastCalledWith({ top: SENT_TOP_PX, behavior: "smooth" });
      expect(frame.viewport.scrollTop).toBe(SENT_TOP_PX);
      // Room below the short reply: the sent turn's top plus a frame, less the log.
      const reserve = view.container.querySelector<HTMLElement>(RESERVE);
      expect(reserve?.style.height).toBe("220px");
      expect(reserve?.previousElementSibling?.getAttribute("data-message-id")).toBe("turn-1:reply");
      view.rerender(<View messages={[...RUNNING_TURN.slice(0, 3), { id: "turn-1:reply", role: "assistant", content: "Rolling out to 5% of hosts." }]} eventsCount={2} isRunning />);
      frame.size.log = 1_100;
      browser.layout(frame.log, frame.content, reserve?.previousElementSibling ?? frame.content);
      expect(frame.scrollTo).toHaveBeenCalledTimes(2);
      expect(frame.viewport.scrollTop).toBe(SENT_TOP_PX);
      expect(reserve?.style.height).toBe("120px");
    });

    it("holds the view while another sender's reply streams in below the reader", () => {
      const browser = stubBrowserLayout();
      const view = render(<View messages={SETTLED_TURN} eventsCount={1} />);
      const frame = browser.frameOf(view);
      browser.layout(frame.log);
      for (const [text, log] of [["Checking", 1_200], ["Checking the canary rollout and its error budget.", 1_500]] as const) {
        view.rerender(<View messages={TEAMMATE_REPLY(text)} eventsCount={2} />);
        frame.size.log = log;
        browser.layout(frame.log, frame.content);
      }
      expect(frame.scrollTo).toHaveBeenCalledTimes(1);
      expect(frame.viewport.scrollTop).toBe(600);
    });

    it("holds the view when the window or the composer resizes", () => {
      const browser = stubBrowserLayout();
      const view = render(<View messages={SETTLED_TURN} eventsCount={1} />);
      const frame = browser.frameOf(view);
      browser.layout(frame.log);
      frame.size.frame = 300;
      browser.layout(frame.viewport);
      Object.defineProperty(frame.footer, "offsetHeight", { configurable: true, value: 160 });
      browser.layout(frame.footer);
      expect(frame.scrollTo).toHaveBeenCalledTimes(1);
      expect(frame.viewport.scrollTop).toBe(600);
    });
  });
});

describe("FleetThreadViewport offline notice", () => {
  // jsdom does no layout, so the held transcript is proven in
  // fleet-thread-anchor.spec.ts. This pins where the notice lives: in the
  // sticky footer's flow, above the composer. Above the thread it moved every
  // row; laid over the history it hid the newest reply from a reader at the
  // bottom.
  it("warns from the composer's footer, in its flow", () => {
    const view = render(<View connectionStatus={CONNECTION_STATUS.OFFLINE} />);
    const notice = view.getByTestId("fleet-connection-notice");
    const footer = view.getByTestId("fleet-chat-footer");
    expect(notice.closest('[data-testid="fleet-chat-footer"]')).toBe(footer);
    expect(notice.closest(".absolute")).toBeNull();
    const composer = view.getByRole("form", { name: "Chat composer" });
    expect(notice.compareDocumentPosition(composer) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });
});

describe("FleetThreadViewport layout", () => {
  // A footer outside the scrolling viewport floated over a blank band and left
  // Send below the clipped root. Where it sits on screen is measured in the
  // browser (fleet-thread.spec.ts); here the structure holds it inside.
  it("keeps the composer footer inside the thread viewport", () => {
    const view = render(<View />);
    const footerElement = view.getByTestId("fleet-chat-footer");
    expect(view.getByTestId("fleet-thread-root").querySelector('[role="presentation"]')?.contains(footerElement)).toBe(true);
  });
  it("places the saved setup inside the log once, with the composer still available", () => {
    const view = render(<View setup={<h2>Setup saved · v1</h2>} />);
    expect(view.getByRole("log").contains(view.getByRole("heading", {name: "Setup saved · v1"}))).toBe(true);
    expect(view.queryAllByText("Setup saved · v1")).toHaveLength(1);
    expect(view.queryByText("Saved v1")).toBeNull();
    expect(view.getByRole("form", {name: "Chat composer"})).toBeTruthy();
  });
  it("replaces the saved receipt in sequence without repeating it above later messages", () => {
    const messages: ThreadMessageLike[] = [
      {id: "before", role: "user", content: "Save this setup"},
      {id: "saved", role: "system", content: "Setup saved · v1"},
      {id: "after", role: "user", content: "Check performance"},
    ];
    const view = render(<View messages={messages} eventsCount={3} setupTurnId="saved" setup={<h2>Setup saved · v1</h2>} />);
    const history = view.getByRole("log").textContent ?? "";
    expect(history.indexOf("Save this setup")).toBeLessThan(history.indexOf("Setup saved · v1"));
    expect(history.indexOf("Setup saved · v1")).toBeLessThan(history.indexOf("Check performance"));
    expect(view.getAllByText("Setup saved · v1")).toHaveLength(1);
  });
});

 it("keeps work outside the composer and shows inline Summary without modifying its text", () => {
  const view = render(<View work={<p>Saved PR evidence</p>} />);
  const composer = view.getByRole("textbox", {name: "Message your fleet…"});
  fireEvent.change(composer, {target: {value: "Keep session expiry unchanged"}});
  expect(view.getByTestId("fleet-chat-footer").textContent).not.toContain("Saved PR evidence");
  expect(view.getByRole("complementary", {name: "Summary"}).textContent).toContain("Saved PR evidence");
  expect(composer).toHaveProperty("value", "Keep session expiry unchanged");
 });

type Frame = ReturnType<ReturnType<typeof stubBrowserLayout>["frameOf"]>;

/**
 * Stands in for a browser's layout: animation frames, the resize observers of
 * each box a step resized, and the viewport's geometry. Install it before the
 * render, since the library observes the viewport as it mounts.
 */
function stubBrowserLayout() {
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
    frameOf(view: RenderResult) {
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
    },
  };
}
