import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { AssistantRuntimeProvider, ThreadPrimitive, useExternalStoreRuntime } from "@assistant-ui/react";
import type { Options } from "react-markdown";

import type { FleetEvent } from "@/lib/streaming/fleet-stream-row";
import { AGENTSFLEET_EVENT_STATUS } from "@/lib/streaming/fleet-stream-row";
import { FleetMarkdown } from "./FleetMarkdown";
import { renderReplyPart, SETTLED_REPLY_STATUS, SPOKEN_REPLY_MAX_CHARS, spokenSummary, type ReplyContext } from "./FleetReplyBody";
import { renderFleetMessage } from "./fleetMessageRenderers";
import { toReplyMessage } from "./fleetReplyMessage";
import { convertEvent } from "./useFleetEventStream";
import {
  BLOCK_SEPARATOR,
  CHUNK_SIZE,
  LARGEST_BLOCK,
  someFlushSplitsAFence,
  STREAMED_ANSWER,
  streamedPrefixes,
} from "@/tests/helpers/fleet-markdown-corpus";

// Every markdown parse the reply runs, sorted by what it parsed: the open tail
// carries the block reader as a second plugin; a finished block and a settled
// reply carry GFM alone.
const parses = vi.hoisted(() => ({ tails: [] as string[], wholes: [] as string[] }));
const TAIL_PLUGIN_COUNT = 2;

vi.mock("react-markdown", async (importOriginal) => {
  const actual = await importOriginal<typeof import("react-markdown")>();
  return {
    ...actual,
    default: (options: Options) => {
      const parsed = options.remarkPlugins?.length === TAIL_PLUGIN_COUNT ? parses.tails : parses.wholes;
      parsed.push(options.children ?? "");
      return actual.default(options);
    },
  };
});

const REPLY: ReplyContext = {
  errored: false,
  running: false,
  queued: false,
  eventId: "e1",
  reasoning: "",
  span: { startedAtMs: null, endedAtMs: null },
};
const RUNNING = "running";
const COMPLETE = "complete";
// The open tail is the two blocks held back, each with the blank line after
// it, plus what one flush appended — never a function of the answer's length.
const TAIL_BOUND = 2 * (LARGEST_BLOCK + BLOCK_SEPARATOR.length) + CHUNK_SIZE;
// Elements the tail can hold at any flush: the two held blocks, and the block
// one chunk can finish and the one it can start.
const OPEN_ELEMENTS = 4;

type PartInfo = Parameters<typeof renderReplyPart>[0];
const REPLY_TEXT = "Opened the PR.";

// One fleet reply through the real renderer, with no transcript around it.
function Transcriptless({ status }: { status: FleetEvent["status"] }) {
  const event: FleetEvent = {
    id: REPLY.eventId, role: "assistant", actor: "fleet", text: "", reply: REPLY_TEXT, outcome: "",
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

function textPart(text: string, running: boolean) {
  const info = { part: { type: "text", text, status: { type: running ? RUNNING : COMPLETE } }, children: null };
  return renderReplyPart(info as unknown as PartInfo, { ...REPLY, running });
}

function elementsOf(root: Element | null): Element[] {
  return [...(root?.children ?? [])];
}

beforeEach(() => {
  parses.tails.length = 0;
  parses.wholes.length = 0;
});

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

describe("a streaming answer", () => {
  it("test_streaming_markdown_reparses_only_the_open_block: 400 flushes of a 20 KB answer with split fences keep finished blocks and settle to a single parse", () => {
    expect(someFlushSplitsAFence()).toBe(true);
    const prefixes = streamedPrefixes();
    const view = render(textPart("", true));
    const body = () => view.container.firstElementChild;
    let finishedMidway: Element[] = [];
    for (const [flush, prefix] of prefixes.entries()) {
      view.rerender(textPart(prefix, true));
      if (flush === prefixes.length / 2) finishedMidway = elementsOf(body()).slice(0, -OPEN_ELEMENTS);
    }

    // No flush parsed more than its open tail, and no finished block was ever
    // parsed twice: the finished blocks, each parsed once, and the last tail
    // are the answer exactly.
    expect(Math.max(...parses.tails.map((tail) => tail.length))).toBeLessThanOrEqual(TAIL_BOUND);
    expect(parses.wholes.join("") + (parses.tails.at(-1) ?? "")).toBe(STREAMED_ANSWER);
    // A block finished halfway is the same element at the end of the stream.
    expect(finishedMidway.length).toBeGreaterThan(0);
    elementsOf(body()).slice(0, finishedMidway.length).forEach((element, index) => {
      expect(element).toBe(finishedMidway[index]);
    });

    // Block by block renders the elements one parse renders.
    const single = render(<FleetMarkdown>{STREAMED_ANSWER}</FleetMarkdown>).container.firstElementChild;
    expect(elementsOf(body()).map((element) => element.outerHTML)).toEqual(elementsOf(single).map((element) => element.outerHTML));

    // Settled, the reply is today's single parse, byte for byte.
    const wholesBeforeSettling = parses.wholes.length;
    view.rerender(textPart(STREAMED_ANSWER, false));
    expect(body()?.outerHTML).toBe(single?.outerHTML);
    expect(parses.wholes.slice(wholesBeforeSettling)).toContain(STREAMED_ANSWER);
  });
});
