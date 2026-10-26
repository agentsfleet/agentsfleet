import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { AssistantRuntimeProvider, ThreadPrimitive, useExternalStoreRuntime } from "@assistant-ui/react";

import type { FleetEvent } from "@/lib/streaming/fleet-stream-row";
import { AGENTSFLEET_EVENT_STATUS } from "@/lib/streaming/fleet-stream-row";
import { renderReplyPart, SETTLED_REPLY_STATUS, SPOKEN_REPLY_MAX_CHARS, spokenSummary, type ReplyContext } from "./FleetReplyBody";
import { renderFleetMessage } from "./fleetMessageRenderers";
import { toReplyMessage } from "./fleetReplyMessage";
import { convertEvent } from "./useFleetEventStream";

const REPLY: ReplyContext = {
  errored: false,
  running: false,
  queued: false,
  eventId: "e1",
  reasoning: "",
  span: { startedAtMs: null, endedAtMs: null },
  content: [],
  startedAtMs: 0,
};
const COMPLETE = "complete";

type PartInfo = Parameters<typeof renderReplyPart>[0];
const REPLY_TEXT = "Opened the PR.";

// One fleet reply through the real renderer, with no transcript around it.
function Transcriptless({ status, text = REPLY_TEXT }: { status: FleetEvent["status"]; text?: string }) {
  const event: FleetEvent = {
    id: REPLY.eventId, role: "assistant", actor: "fleet", text: "", reply: text, outcome: "",
    failureLabel: null, failureDetail: null, createdAt: new Date(0), status,
  };
  const runtime = useExternalStoreRuntime<FleetEvent>({
    isRunning: false,
    messages: [event],
    convertMessage: (row) => toReplyMessage(convertEvent(row), row),
    onNew: async () => {},
  });
  return (
    <AssistantRuntimeProvider runtime={runtime}>
      <ThreadPrimitive.Messages>{renderFleetMessage}</ThreadPrimitive.Messages>
    </AssistantRuntimeProvider>
  );
}

afterEach(cleanup);

describe("renderReplyPart", () => {
  it("should render nothing for a part type the reply never carries", () => {
    // The converter emits reasoning, tool-call and text only. Anything else
    // the library could hand back renders nothing, never its fallback UI.
    const image = { part: { type: "image", image: "https://example.test/x.png", status: { type: COMPLETE } }, children: null };
    const { container } = render(renderReplyPart(image as unknown as PartInfo, REPLY));
    expect(container.childNodes).toHaveLength(0);
  });
});

describe("a reply's settling", () => {
  it("keeps selected reply text attached when the run completes", () => {
    const view = render(<Transcriptless status={AGENTSFLEET_EVENT_STATUS.RECEIVED} />);
    const paragraph = screen.getByText(REPLY_TEXT);
    const range = document.createRange();
    range.selectNodeContents(paragraph);
    const selection = window.getSelection();
    selection?.removeAllRanges();
    selection?.addRange(range);

    view.rerender(<Transcriptless status={AGENTSFLEET_EVENT_STATUS.PROCESSED} />);

    expect(paragraph.isConnected).toBe(true);
    expect(screen.getByText(REPLY_TEXT)).toBe(paragraph);
    expect(selection?.toString()).toBe(REPLY_TEXT);
  });

  it("settles with nothing to announce to when no transcript is around it", () => {
    const view = render(<Transcriptless status={AGENTSFLEET_EVENT_STATUS.RECEIVED} />);
    view.rerender(<Transcriptless status={AGENTSFLEET_EVENT_STATUS.PROCESSED} />);
    expect(screen.getByText(REPLY_TEXT)).toBeTruthy();
    expect(screen.queryByTestId(SETTLED_REPLY_STATUS)).toBeNull();
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
  const view = render(<Transcriptless status={AGENTSFLEET_EVENT_STATUS.RECEIVED} text={first} />);
  const paragraph = screen.getByText("First paragraph.");
  const final = "First paragraph.\n\nRead [docs][guide].\n\nFinal answer.\n\n[guide]: https://example.com/guide";
  view.rerender(<Transcriptless status={AGENTSFLEET_EVENT_STATUS.PROCESSED} text={final} />);
  expect(screen.getByText("First paragraph.")).toBe(paragraph);
  expect(screen.getByRole("link", { name: "docs" }).getAttribute("href")).toBe("https://example.com/guide");
  expect(screen.getByText("Final answer.")).toBeTruthy();
  expect(screen.queryByText("Still writing")).toBeNull();
});
