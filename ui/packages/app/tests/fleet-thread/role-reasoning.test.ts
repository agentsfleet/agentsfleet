import { ev, mockStream, renderThread, threadElement } from "./harness";
import { describe, expect, it, vi } from "vitest";
import { act, fireEvent, renderHook, screen } from "@testing-library/react";
import { FIRST_VISIBLE_MEASURE, useFirstVisiblePaint } from "@/components/domain/useFirstVisiblePaint";

const THOUGHT = /^Thought/;
const THINKING = /^Thinking/;
const SPAN_START = 1_000;
const SPAN_END = 9_500;

/**
 * The typed reasoning, as the reply's Thought chip.
 *
 * The stream decoder separates reasoning from the answer before rendering; the
 * reply carries it as a reasoning part. These pin the chip's two states, the
 * row's span reaching it, and the operator's control over the fold.
 */
describe("FleetThread — reasoning disclosure", () => {
  it("folds finished reasoning away and shows the answer", () => {
    mockStream([
      ev({
        role: "assistant",
        actor: "fleet",
        reasoning: "weighing the blast radius",
        reply: "Opened the PR.",
      }),
    ]);
    renderThread();

    // Closed once the answer lands: by then it is working-out nobody asked for.
    expect(screen.getByRole("button", { name: THOUGHT })).toBeTruthy();
    expect(screen.queryByRole("button", { name: THINKING })).toBeNull();
    expect(screen.queryByText("weighing the blast radius")).toBeNull();
    expect(screen.getByText("Opened the PR.")).toBeTruthy();
  });

  it("carries the row's reasoning span to the folded chip", () => {
    mockStream([
      ev({
        role: "assistant",
        actor: "fleet",
        reasoning: "weighing the blast radius",
        reply: "Opened the PR.",
        reasoningStartedAtMs: SPAN_START,
        reasoningEndedAtMs: SPAN_END,
      }),
    ]);
    renderThread();
    expect(screen.getByRole("button", { name: THOUGHT }).textContent).toBe("Thought · 8.5s");
  });

  it("opens the fold while the block is still arriving", () => {
    // The decoder marks an open thinking block until its closing tag arrives.
    mockStream([
      ev({ role: "assistant", actor: "fleet", status: "received", reasoning: "still weighing it", thinking: true }),
    ]);
    renderThread();

    expect(screen.getByRole("button", { name: THINKING })).toBeTruthy();
    expect(screen.getByText("still weighing it", { selector: "p" })).toBeTruthy();
  });

  it("lets the operator open a finished fold", () => {
    mockStream([
      ev({
        role: "assistant",
        actor: "fleet",
        reasoning: "checked the diff twice",
        reply: "Done.",
      }),
    ]);
    renderThread();

    fireEvent.click(screen.getByRole("button", { name: THOUGHT }));
    expect(screen.getByText("checked the diff twice")).toBeTruthy();
  });

  it("uses quiet reasoning text and hides tool payloads from the disclosure", () => {
    mockStream([
      ev({
        role: "assistant",
        actor: "fleet",
        reasoning: "Recalling. Done.",
        reply: "Remembered.",
      }),
    ]);
    renderThread();
    fireEvent.click(screen.getByRole("button", { name: THOUGHT }));
    const reasoning = screen.getByText("Recalling. Done.");
    // `text-dim`, not `text-subtle`: subtle holds its chroma and reads as a
    // second colour beside the answer; dim is the voice a reader can skip.
    expect(reasoning.className).toContain("text-text-dim");
    expect(screen.queryByText(/private/)).toBeNull();
    expect(screen.getByText("Remembered.").closest(".text-text-chat")).toBeTruthy();
  });

  it("renders no fold at all when the model reasoned nowhere", () => {
    mockStream([ev({ role: "assistant", actor: "fleet", reply: "Opened the PR." })]);
    renderThread();

    expect(screen.queryByRole("button", { name: THOUGHT })).toBeNull();
    expect(screen.queryByRole("button", { name: THINKING })).toBeNull();
  });

  it("shows the fold but no answer while only reasoning has arrived", () => {
    // A reasoning-only delta opens the fold without inventing an answer.
    mockStream([
      ev({
        role: "assistant",
        actor: "fleet",
        status: "received",
        reasoning: "reading the diff",
        thinking: true,
      }),
    ]);
    renderThread();

    expect(screen.getByRole("button", { name: THINKING })).toBeTruthy();
    expect(screen.getByText("reading the diff", { selector: "p" })).toBeTruthy();
  });

  it("withholds Copy reply while the durable final reply is being recovered", () => {
    mockStream([ev({ role: "assistant", actor: "fleet", reply: "Partial draft", replyRecovering: true })]);
    renderThread();
    expect(screen.getByText("Loading final reply; retrying if needed…")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Copy reply" })).toBeNull();
  });

  it("measures local submission through the first browser paint of reply text", () => {
    let nextFrame = 0;
    const frames = new Map<number, FrameRequestCallback>();
    vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
      const id = ++nextFrame;
      frames.set(id, callback);
      return id;
    });
    vi.stubGlobal("cancelAnimationFrame", (id: number) => { frames.delete(id); });
    const measure = vi.spyOn(performance, "measure").mockImplementation(() => ({} as PerformanceMeasure));
    try {
      mockStream([ev({ role: "assistant", actor: "fleet", reply: "First visible words", submittedAtMs: 1 })]);
      renderThread();
      expect(measure).not.toHaveBeenCalled();
      act(() => {
        for (const [id, callback] of [...frames]) { frames.delete(id); callback(0); }
      });
      expect(measure).not.toHaveBeenCalled();
      act(() => {
        for (const [id, callback] of [...frames]) { frames.delete(id); callback(0); }
      });
      expect(measure).toHaveBeenCalledWith(FIRST_VISIBLE_MEASURE, expect.objectContaining({ start: 1 }));
    } finally {
      measure.mockRestore();
      vi.unstubAllGlobals();
    }
  });

  it("measures a tool-first response as the first visible fleet activity", () => {
    let nextFrame = 0;
    const frames = new Map<number, FrameRequestCallback>();
    vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
      const id = ++nextFrame;
      frames.set(id, callback);
      return id;
    });
    vi.stubGlobal("cancelAnimationFrame", (id: number) => { frames.delete(id); });
    const measure = vi.spyOn(performance, "measure").mockImplementation(() => ({} as PerformanceMeasure));
    try {
      mockStream([ev({ role: "assistant", actor: "fleet", status: "received", submittedAtMs: 1,
        tools: [{ name: "memory_recall", startedAtMs: 1, ms: null, done: false }] })]);
      renderThread();
      expect(screen.getByText("memory_recall")).toBeTruthy();
      act(() => { for (const [id, callback] of [...frames]) { frames.delete(id); callback(0); } });
      act(() => { for (const [id, callback] of [...frames]) { frames.delete(id); callback(0); } });
      expect(measure).toHaveBeenCalledWith(FIRST_VISIBLE_MEASURE, expect.objectContaining({ start: 1 }));
    } finally {
      measure.mockRestore();
      vi.unstubAllGlobals();
    }
  });

  it("measures a user turn only once when its tool-first reply gains an answer", () => {
    let nextFrame = 0;
    const frames = new Map<number, FrameRequestCallback>();
    vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
      const id = ++nextFrame;
      frames.set(id, callback);
      return id;
    });
    vi.stubGlobal("cancelAnimationFrame", (id: number) => { frames.delete(id); });
    const measure = vi.spyOn(performance, "measure").mockImplementation(() => ({} as PerformanceMeasure));
    const event = ev({ id: "evt_tool_then_answer", role: "user", actor: "operator", text: "Check memory",
      status: "received", submittedAtMs: 1, tools: [{ name: "memory_recall", startedAtMs: 1, ms: null, done: false }] });
    try {
      mockStream([event]);
      const view = renderThread();
      act(() => { for (const [id, callback] of [...frames]) { frames.delete(id); callback(0); } });
      act(() => { for (const [id, callback] of [...frames]) { frames.delete(id); callback(0); } });
      expect(measure).toHaveBeenCalledTimes(1);
      mockStream([{ ...event, reply: "Answer", status: "processed" }]);
      view.rerender(threadElement());
      act(() => { for (const [id, callback] of [...frames]) { frames.delete(id); callback(0); } });
      act(() => { for (const [id, callback] of [...frames]) { frames.delete(id); callback(0); } });
      expect(screen.getByText("Answer")).toBeTruthy();
      expect(measure).toHaveBeenCalledTimes(1);
    } finally {
      measure.mockRestore();
      vi.unstubAllGlobals();
    }
  });

  it("deduplicates simultaneous mounts while distinguishing a second local submission", () => {
    let nextFrame = 0;
    const frames = new Map<number, FrameRequestCallback>();
    vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
      const id = ++nextFrame;
      frames.set(id, callback);
      return id;
    });
    vi.stubGlobal("cancelAnimationFrame", (id: number) => { frames.delete(id); });
    const measure = vi.spyOn(performance, "measure").mockImplementation(() => ({} as PerformanceMeasure));
    try {
      // Three mounts: the same event and submission twice (one paint), and a
      // second submission of it (its own paint).
      renderHook(() => useFirstVisiblePaint("evt_same_id_two_fleets", 2, true));
      renderHook(() => useFirstVisiblePaint("evt_same_id_two_fleets", 2, true));
      renderHook(() => useFirstVisiblePaint("evt_same_id_two_fleets", 3, true));
      act(() => { for (const [id, callback] of [...frames]) { frames.delete(id); callback(0); } });
      act(() => { for (const [id, callback] of [...frames]) { frames.delete(id); callback(0); } });
      expect(measure).toHaveBeenCalledTimes(2);
    } finally {
      measure.mockRestore();
      vi.unstubAllGlobals();
    }
  });
});
