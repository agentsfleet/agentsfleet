// @vitest-environment node
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { DRAFT_CONTEXT_EVENT, DRAFT_DELTA_EVENT, DRAFT_RESYNC_EVENT } from "@/lib/api/draft-events";
import { DraftClientFixture } from "./helpers/draft-client-fixture";
import { FakeEventSource } from "./helpers/fake-event-source";
import { DRAFT, MESSAGE, response } from "./helpers/draft-fixtures";

const fixture = new DraftClientFixture();
const stream = "0199a801-1000-7000-8000-000000000011";
const start = {generation: 1, state: "summarizing"};
const end = {...start, state: "resuming"};
const frame = (progress: unknown) => JSON.stringify({draft_id: DRAFT.id, sequence: MESSAGE.sequence, stream_id: stream, progress});
beforeEach(() => fixture.mount());
afterEach(() => fixture.dispose());

it("delivers private context status, retires it on a real answer and clears ownership", async () => {
  fixture.override = ({path}) => path.endsWith("/messages")
    ? response({items: [{...MESSAGE, state: "running"}], next_cursor: null}) : undefined;
  const client = await fixture.ready(true);
  const source = FakeEventSource.instances.at(-1)!;
  source.emitNamed(DRAFT_CONTEXT_EVENT, frame(start));
  expect(client.getSnapshot().contextProgress).toEqual({sequence: MESSAGE.sequence, progress: start});
  source.emitNamed(DRAFT_CONTEXT_EVENT, frame(end));
  expect(client.getSnapshot().contextProgress?.progress).toEqual(end);
  source.emitNamed(DRAFT_CONTEXT_EVENT, frame(start));
  expect(client.getSnapshot().contextProgress?.progress).toEqual(end);
  source.emitNamed(DRAFT_DELTA_EVENT, JSON.stringify({draft_id: DRAFT.id, sequence: MESSAGE.sequence,
    stream_id: stream, stream_seq: 1, stream_start: true, text_kind: "answer", text: "Resumed answer"}));
  expect(client.getSnapshot().contextProgress).toBeNull();
  expect(client.getSnapshot().liveReply?.text).toBe("Resumed answer");
  source.emitNamed(DRAFT_CONTEXT_EVENT, frame({...start, generation: 2}));
  expect(client.getSnapshot().contextProgress?.progress.generation).toBe(2);
  client.dispose();
  expect(client.getSnapshot().contextProgress).toBeUndefined();
  source.emitNamed(DRAFT_CONTEXT_EVENT, frame({...start, generation: 3}));
  expect(client.getSnapshot().draft).toBeNull();
  expect(client.getSnapshot().contextProgress).toBeUndefined();
});

it("clears status on a stream gap and never allows late metadata to revive completed history", async () => {
  let completed = false;
  fixture.override = ({path}) => path.endsWith("/messages") ? response({items: [
    {...MESSAGE, state: completed ? "completed" : "running"},
    ...(completed ? [{...MESSAGE, role: "assistant", text: "Durable answer"}] : []),
  ], next_cursor: null}) : undefined;
  const client = await fixture.ready(true);
  const source = FakeEventSource.instances.at(-1)!;
  source.emitNamed(DRAFT_CONTEXT_EVENT, frame(start));
  source.emitNamed(DRAFT_RESYNC_EVENT, "{}");
  expect(client.getSnapshot().contextProgress).toBeNull();
  source.emitNamed(DRAFT_CONTEXT_EVENT, frame(end));
  expect(client.getSnapshot().contextProgress?.progress).toEqual(end);
  completed = true;
  source.emitNamed("message.completed", JSON.stringify({draft_id: DRAFT.id}));
  await vi.waitFor(() => expect(client.getSnapshot().messages.at(-1)?.text).toBe("Durable answer"));
  expect(client.getSnapshot().contextProgress).toBeNull();
  source.emitNamed(DRAFT_CONTEXT_EVENT, frame({...start, generation: 2}));
  expect(client.getSnapshot().contextProgress).toBeNull();
});
