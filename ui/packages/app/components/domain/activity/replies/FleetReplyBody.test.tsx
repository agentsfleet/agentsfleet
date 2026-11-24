import { afterEach, describe, expect, it } from "vitest";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { AssistantRuntimeProvider, ThreadPrimitive, useExternalStoreRuntime } from "@assistant-ui/react";

import type { FleetTurn } from "@/lib/streaming/fleet-stream-row";
import { AGENTSFLEET_TURN_STATUS } from "@/lib/streaming/fleet-stream-row";
import type { ReplyContext } from "./fleet-reply-content";
import { renderReplyPart } from "./FleetReplyParts";
import { SETTLED_REPLY_STATUS, SPOKEN_REPLY_MAX_CHARS, SettledReplyStatus, spokenSummary } from "./FleetReplyAnnouncements";
import { renderFleetMessage } from "../messages/fleetMessageRenderers";
import { toReplyMessage } from "./fleetReplyMessage";
import { convertTurn } from "../../conversation/stream/useFleetTurnStream";

const REPLY: ReplyContext = {
  running: false,
  queued: false,
  turnId: "e1",
  reasoning: "",
  span: { startedAtMs: null, endedAtMs: null },
  content: [],
};
const COMPLETE = "complete";

type PartInfo = Parameters<typeof renderReplyPart>[0];
const REPLY_TEXT = "Opened the PR.";

type ReplyFields = Pick<FleetTurn, "tokens" | "wallMs" | "costNanos" | "replyRecovering" | "durableTaskActivity">;

// One fleet reply through the real renderer, with no transcript around it.
function Transcriptless({ status, text = REPLY_TEXT, restarted = false, announce = false, attempt = "2", fields }: { status: FleetTurn["status"]; text?: string; restarted?: boolean; announce?: boolean; attempt?: string; fields?: Partial<ReplyFields> }) {
  const turn: FleetTurn = {
    id: REPLY.turnId, role: "assistant", actor: "fleet", text: "", reply: text, outcome: "",
    failureLabel: null, failureDetail: null, createdAt: new Date(0), status, runRestarted: restarted ? true : undefined,
    ...(restarted ? { restartAttempt: attempt } : {}),
    ...fields,
  };
  const runtime = useExternalStoreRuntime<FleetTurn>({
    isRunning: false,
    messages: [turn],
    convertMessage: (row) => toReplyMessage(convertTurn(row), row),
    onNew: async () => {},
  });
  const reply = (
    <AssistantRuntimeProvider runtime={runtime}>
      <ThreadPrimitive.Messages>{renderFleetMessage}</ThreadPrimitive.Messages>
    </AssistantRuntimeProvider>
  );
  return announce ? <SettledReplyStatus>{reply}</SettledReplyStatus> : reply;
}

afterEach(cleanup);

describe("renderReplyPart", () => {
  it("keeps an image without a preview readable without interpreting its payload", () => {
    const image: PartInfo = { part: { type: "image", image: "javascript:alert(1)", status: { type: COMPLETE } }, children: null };
    const { container } = render(renderReplyPart(image, REPLY));
    expect(screen.getByText("Activity details · Preview unavailable.")).toBeTruthy();
    expect(container.querySelector("img, a, script")).toBeNull();
    expect(container.textContent).not.toContain("javascript:");
  });
});

describe("a reply's settling", () => {
  it("announces each observed restart once while keeping streamed output quiet", () => {
    const status = AGENTSFLEET_TURN_STATUS.RECEIVED;
    const view = render(<Transcriptless status={status} announce />);
    const spoken = screen.getByTestId(SETTLED_REPLY_STATUS);
    expect(spoken.textContent).toBe("");
    view.rerender(<Transcriptless status={status} announce restarted />);
    expect(spoken.textContent).toBe("Run restarted.");
    const first = spoken.firstChild;
    view.rerender(<Transcriptless status={status} text="new streamed text" announce restarted />);
    expect(spoken.firstChild).toBe(first);
    view.rerender(<Transcriptless status={status} announce restarted attempt="3" />);
    expect(spoken.textContent).toBe("Run restarted.");
    expect(spoken.firstChild).not.toBe(first);
  });

  it("shows a restart on its existing reply and keeps the marker after settlement", () => {
    const view = render(<Transcriptless status={AGENTSFLEET_TURN_STATUS.RECEIVED} restarted />);
    expect(screen.getByText("Run restarted")).toBeTruthy();
    view.rerender(<Transcriptless status={AGENTSFLEET_TURN_STATUS.PROCESSED} restarted />);
    expect(screen.getByText("Run restarted")).toBeTruthy();
    expect(screen.getByText(REPLY_TEXT)).toBeTruthy();
  });

  it("keeps selected reply text attached when the run completes", () => {
    const view = render(<Transcriptless status={AGENTSFLEET_TURN_STATUS.RECEIVED} />);
    const paragraph = screen.getByText(REPLY_TEXT);
    const range = document.createRange();
    range.selectNodeContents(paragraph);
    const selection = window.getSelection();
    selection?.removeAllRanges();
    selection?.addRange(range);

    view.rerender(<Transcriptless status={AGENTSFLEET_TURN_STATUS.PROCESSED} />);

    expect(paragraph.isConnected).toBe(true);
    expect(screen.getByText(REPLY_TEXT)).toBe(paragraph);
    expect(selection?.toString()).toBe(REPLY_TEXT);
  });

  it("settles with nothing to announce to when no transcript is around it", () => {
    const view = render(<Transcriptless status={AGENTSFLEET_TURN_STATUS.RECEIVED} />);
    view.rerender(<Transcriptless status={AGENTSFLEET_TURN_STATUS.PROCESSED} />);
    expect(screen.getByText(REPLY_TEXT)).toBeTruthy();
    expect(screen.queryByTestId(SETTLED_REPLY_STATUS)).toBeNull();
  });

  it("settles a plain reply with copy controls and without a metering receipt", () => {
    render(<Transcriptless status={AGENTSFLEET_TURN_STATUS.PROCESSED} fields={{tokens: 290, wallMs: 693, costNanos: "100000"}} />);
    expect(screen.getByText(REPLY_TEXT)).toBeTruthy();
    expect(screen.getByRole("button", {name: "Copy reply"})).toBeTruthy();
    expect(screen.queryByRole("button", {name: "Run details"})).toBeNull();
    expect(screen.queryByText(/Worked for|Work completed|Reported cost/)).toBeNull();
  });

  it("opens completed task history after settlement, without usage figures", () => {
    const durableTaskActivity: FleetTurn["durableTaskActivity"] = {truncated: false, items: [{kind: "durable_task_decision_completed", progress: {
      activity_id: "019f0000-0000-7000-8000-000000000001", durable_task_id: null, source_turn_id: null,
      operation: "goal", phase: "recorded", provider: null, model: null, completed: 1, total: 1,
    }}]};
    const view = render(<Transcriptless status={AGENTSFLEET_TURN_STATUS.RECEIVED} fields={{durableTaskActivity}} />);
    expect(screen.queryByRole("button", {name: "Task activity"})).toBeNull();
    view.rerender(<Transcriptless status={AGENTSFLEET_TURN_STATUS.PROCESSED} text="" fields={{durableTaskActivity, replyRecovering: true}} />);
    expect(screen.getByText(/Loading final reply/)).toBeTruthy();
    expect(screen.queryByRole("button", {name: "Task activity"})).toBeNull();
    view.rerender(<Transcriptless status={AGENTSFLEET_TURN_STATUS.PROCESSED} fields={{durableTaskActivity}} />);
    expect(screen.queryByText("Assessment recorded · goal progress")).toBeNull();
    fireEvent.click(screen.getByRole("button", {name: "Task activity"}));
    const history = within(screen.getByRole("dialog", {name: "Task activity"}));
    expect(history.getAllByText("Assessment recorded · goal progress")).toHaveLength(2);
    expect(history.queryByText("Reported cost")).toBeNull();
  });
});

describe("spokenSummary", () => {
  it("reads markdown as plain words", () => {
    const markdown = [
      "# Deployed",
      "",
      "> Ran **all** the `checks` on _main_ ~~twice~~.",
      "",
      "- Opened [the PR](https://example.test/pr/1) ![chart](c.png)",
      "1. Kept `fleet_id` and __init__ as written",
      "",
      "---",
      "```ts",
      "const ok = true;",
      "```",
      "| a | b |",
    ].join("\n");
    expect(spokenSummary(markdown)).toBe(
      "Deployed Ran all the checks on main twice. Opened the PR chart Kept fleet_id and init as written const ok = true; a b",
    );
  });

  it("bounds a long reply at the cap", () => {
    const long = "word ".repeat(SPOKEN_REPLY_MAX_CHARS);
    const spoken = spokenSummary(long);
    expect(spoken).toBe(`${long.slice(0, SPOKEN_REPLY_MAX_CHARS)}…`);
    expect(spokenSummary("  Done.  ")).toBe("Done.");
  });
});

it("resolves a late reference without replacing earlier reply text", () => {
  const first = "First paragraph.\n\nRead [docs][guide].\n\nStill writing";
  const view = render(<Transcriptless status={AGENTSFLEET_TURN_STATUS.RECEIVED} text={first} />);
  const paragraph = screen.getByText("First paragraph.");
  const final = "First paragraph.\n\nRead [docs][guide].\n\nFinal answer.\n\n[guide]: https://example.com/guide";
  view.rerender(<Transcriptless status={AGENTSFLEET_TURN_STATUS.PROCESSED} text={final} />);
  expect(screen.getByText("First paragraph.")).toBe(paragraph);
  expect(screen.getByRole("link", { name: "docs" }).getAttribute("href")).toBe("https://example.com/guide");
  expect(screen.getByText("Final answer.")).toBeTruthy();
  expect(screen.queryByText("Still writing")).toBeNull();
});
