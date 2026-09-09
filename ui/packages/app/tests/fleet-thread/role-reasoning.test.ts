import { ev, mockStream, renderThread } from "./harness";
import { describe, expect, it } from "vitest";
import { fireEvent, screen } from "@testing-library/react";

/**
 * The `<think>` disclosure.
 *
 * Some models reason out loud inline. The durable row keeps only the answer, so
 * before the split the same turn read one way live and another way after a
 * navigation — a paragraph of the model talking to itself mid-stream, gone on
 * reload. These pin the two states the fold has and the operator's control over
 * it.
 */
describe("FleetThread — reasoning disclosure", () => {
  it("folds finished reasoning away and shows the answer", () => {
    mockStream([
      ev({
        role: "assistant",
        actor: "fleet",
        reply: "<think>weighing the blast radius</think>Opened the PR.",
      }),
    ]);
    renderThread();

    // Closed once the answer lands: by then it is working-out nobody asked for.
    expect(screen.getByText("Reasoning")).toBeTruthy();
    expect(screen.queryByText("Thinking…")).toBeNull();
    expect(screen.getByText("Opened the PR.")).toBeTruthy();
  });

  it("opens the fold while the block is still arriving", () => {
    // An unclosed `<think>` IS the model thinking this frame — watching it is
    // the only signal there is during a long turn.
    mockStream([
      ev({ role: "assistant", actor: "fleet", reply: "<think>still weighing it" }),
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
        reply: "<think>checked the diff twice</think>Done.",
      }),
    ]);
    renderThread();

    fireEvent.click(screen.getByText("Reasoning"));
    expect(screen.getByText("checked the diff twice")).toBeTruthy();
  });

  it("renders no fold at all when the model reasoned nowhere", () => {
    mockStream([ev({ role: "assistant", actor: "fleet", reply: "Opened the PR." })]);
    renderThread();

    expect(screen.queryByText("Reasoning")).toBeNull();
    expect(screen.queryByText("Thinking…")).toBeNull();
  });

  it("shows the fold but no answer while only reasoning has arrived", () => {
    // Mid-stream the model has opened `<think>` and said nothing else yet. The
    // reply is NOT empty — so the working indicator does not apply — but the
    // answer is, and an empty bubble under the fold would read as a reply the
    // fleet never gave.
    mockStream([
      ev({
        role: "assistant",
        actor: "fleet",
        status: "received",
        reply: "<think>reading the diff",
      }),
    ]);
    renderThread();

    expect(screen.getByText("Thinking…")).toBeTruthy();
    expect(screen.getByText("reading the diff")).toBeTruthy();
  });
});
