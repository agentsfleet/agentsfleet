import { ev, mockStream, renderThread, threadElement, toThreadMessage } from "./harness";
import { describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { createElement, Fragment } from "react";
import type { MessageState } from "@assistant-ui/react";
import { FleetReply } from "@/components/domain/FleetReplyBody";

/**
 * The typed reasoning disclosure.
 *
 * The stream decoder separates reasoning from the answer before rendering.
 * These pin the two states of the fold and the operator's control over it.
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
    expect(screen.getByText("Reasoning")).toBeTruthy();
    expect(screen.queryByText("Thinking…")).toBeNull();
    expect(screen.getByText("Opened the PR.")).toBeTruthy();
  });

  it("opens the fold while the block is still arriving", () => {
    // The decoder marks an open thinking block until its closing tag arrives.
    mockStream([
      ev({ role: "assistant", actor: "fleet", reasoning: "still weighing it", thinking: true }),
    ]);
    renderThread();

    expect(screen.getByText("Thinking…")).toBeTruthy();
    expect(screen.getByText("still weighing it")).toBeTruthy();
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

    fireEvent.click(screen.getByText("Reasoning"));
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
    fireEvent.click(screen.getByText("Reasoning"));
    const reasoning = screen.getByText("Recalling. Done.");
    expect(reasoning.className).toContain("text-text-subtle");
    expect(screen.queryByText(/private/)).toBeNull();
    expect(screen.getByText("Remembered.").closest(".text-text-chat")).toBeTruthy();
  });

  it("renders no fold at all when the model reasoned nowhere", () => {
    mockStream([ev({ role: "assistant", actor: "fleet", reply: "Opened the PR." })]);
    renderThread();

    expect(screen.queryByText("Reasoning")).toBeNull();
    expect(screen.queryByText("Thinking…")).toBeNull();
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

    expect(screen.getByText("Thinking…")).toBeTruthy();
    expect(screen.getByText("reading the diff")).toBeTruthy();
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
      expect(measure).toHaveBeenCalledWith("agentsfleet.chat.submit_to_first_visible", expect.objectContaining({ start: 1 }));
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
        tools: [{ name: "memory_recall", ms: null, done: false }] })]);
      renderThread();
      expect(screen.getByText("memory_recall")).toBeTruthy();
      act(() => { for (const [id, callback] of [...frames]) { frames.delete(id); callback(0); } });
      act(() => { for (const [id, callback] of [...frames]) { frames.delete(id); callback(0); } });
      expect(measure).toHaveBeenCalledWith("agentsfleet.chat.submit_to_first_visible", expect.objectContaining({ start: 1 }));
    } finally {
      measure.mockRestore();
      vi.unstubAllGlobals();
    }
  });

  it("measures a user turn only once when its tool-first reply splits into an answer row", () => {
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
      status: "received", submittedAtMs: 1, tools: [{ name: "memory_recall", ms: null, done: false }] });
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
    const message = (submittedAtMs: number) => toThreadMessage(ev({
      id: "evt_same_id_two_fleets", role: "assistant", actor: "fleet", reply: "Visible",
      submittedAtMs,
    })) as unknown as MessageState;
    try {
      render(createElement(Fragment, null,
        createElement(FleetReply, { message: message(2), senderLabel: "Fleet A", tools: [], status: "processed" }),
        createElement(FleetReply, { message: message(2), senderLabel: "Fleet A", tools: [], status: "processed" }),
        createElement(FleetReply, { message: message(3), senderLabel: "Fleet B", tools: [], status: "processed" }),
      ));
      act(() => { for (const [id, callback] of [...frames]) { frames.delete(id); callback(0); } });
      act(() => { for (const [id, callback] of [...frames]) { frames.delete(id); callback(0); } });
      expect(measure).toHaveBeenCalledTimes(2);
    } finally {
      measure.mockRestore();
      vi.unstubAllGlobals();
    }
  });
});
