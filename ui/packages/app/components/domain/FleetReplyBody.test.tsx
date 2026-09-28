import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render } from "@testing-library/react";

import { renderReplyPart, type ReplyContext } from "./FleetReplyBody";

const REPLY: ReplyContext = {
  errored: false,
  running: false,
  queued: false,
  eventId: "e1",
  reasoning: "",
  span: { startedAtMs: null, endedAtMs: null },
};

afterEach(cleanup);

describe("renderReplyPart", () => {
  it("should render nothing for a part type the reply never carries", () => {
    // The converter emits reasoning, tool-call and text only. Anything else
    // the library could hand back renders nothing, never its fallback UI.
    const image = { part: { type: "image", image: "https://example.test/x.png", status: { type: "complete" } }, children: null };
    const { container } = render(renderReplyPart(image as unknown as Parameters<typeof renderReplyPart>[0], REPLY));
    expect(container.childNodes).toHaveLength(0);
  });
});
