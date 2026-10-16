import { expect, test } from "bun:test";
import { Effect } from "effect";
import { ChatActions } from "../../src/commands/chat/actions.ts";
import { DraftSession, DRAFT_SOURCE } from "../../src/commands/chat/session.ts";
import { SetupSession } from "../../src/commands/chat/setup-session.ts";
import { MESSAGE_ROLE, MESSAGE_STATE } from "../../src/commands/chat/protocol.ts";
import { outputDouble } from "../helpers-output-double.ts";
import { ACCEPTED, DRAFT, DRAFT_ID, REVIEW, FLEET, FLEET_ID, MODEL, MESSAGE, PRIVATE_TEXT, PUBLICATION, withApi } from "./fixture.ts";

function capture() {
  const rendered: string[] = [];
  const emit = (text: string) => Effect.sync(() => { rendered.push(text); });
  return {rendered, output: {...outputDouble(), info: emit, warn: emit, success: emit}};
}

test("Save, Apply, and publication require a displayed review and ordinary chat performs none of them", async () => {
  const mutations: string[] = [];
  await withApi((request) => {
    const path = new URL(request.url).pathname;
    if (request.method === "POST") mutations.push(path);
    if (path.endsWith("/messages")) return Response.json({items: [MESSAGE,
      {...MESSAGE, role: MESSAGE_ROLE.assistant, text: "A reviewed proposal is available"}], next_cursor: null});
    return Response.json({...DRAFT, accepted: ACCEPTED});
  }, async (transport) => {
    const {output, rendered} = capture();
    const session = new DraftSession(transport, MODEL, {kind: DRAFT_SOURCE.blank});
    const actions = new ChatActions(session, new SetupSession(transport), output, () => Effect.void);
    await Effect.runPromise(Effect.flip(actions.save()));
    await Effect.runPromise(Effect.flip(actions.apply()));
    await Effect.runPromise(Effect.flip(actions.publish("Argo")));
    expect(mutations).toEqual([]);
    await Effect.runPromise(actions.message(PRIVATE_TEXT));
    expect(mutations).toHaveLength(1);
    expect(mutations[0]).toEndWith("/drafts");
    expect(rendered.join("\n")).toContain("A reviewed proposal");
    expect(actions.partial).toBe(false);
    expect(actions.id).toBe(DRAFT_ID);
    actions.clear();
    expect(actions.id).toBeUndefined();
  });
});

test("a displayed review binds Save and partial setup remains a non-success result", async () => {
  const calls: string[] = [];
  await withApi((request) => {
    const path = new URL(request.url).pathname;
    calls.push(path);
    return Response.json(path.endsWith("/fleets") ? {...FLEET, status: "installing", setup: {state: "waiting", checks: [
      {kind: "wallet", subject: "owner", state: "pending", next_action: {kind: "billing", href: "/billing"}},
    ]}} : DRAFT);
  }, async (transport) => {
    const {output, rendered} = capture();
    const actions = new ChatActions(new DraftSession(transport, MODEL, {kind: DRAFT_SOURCE.blank}, DRAFT_ID),
      new SetupSession(transport), output, () => Effect.void);
    await Effect.runPromise(actions.review());
    expect(rendered.join("\n")).toContain(REVIEW.effective.definition.instructions_md);
    expect(rendered.join("\n")).toContain("Required access");
    await Effect.runPromise(actions.save());
    expect(actions.partial).toBe(true);
    expect(rendered.join("\n")).toContain("/billing");
    expect(calls.filter((path) => path.endsWith("/fleets"))).toHaveLength(1);
  });
});

test("Apply uses the setup version matching the displayed improvement review", async () => {
  const tags: (string | null)[] = [];
  await withApi((request) => {
    const path = new URL(request.url).pathname;
    if (path.endsWith("/revisions")) {
      tags.push(request.headers.get("If-Match"));
      return Response.json({fleet_id: FLEET_ID, setup_revision: "2", setup_etag: '"setup-2"', etag: FLEET.etag, schedules: []});
    }
    return Response.json(path.endsWith(`/${FLEET_ID}`) ? FLEET : {...DRAFT, linked_fleet_id: FLEET_ID,
      review: {...REVIEW, effective: {...REVIEW.effective, target_setup_revision: FLEET.setup_revision}}});
  }, async (transport) => {
    const actions = new ChatActions(new DraftSession(transport, MODEL, {kind: DRAFT_SOURCE.blank}, DRAFT_ID),
      new SetupSession(transport), capture().output, () => Effect.void);
    await Effect.runPromise(actions.review());
    await Effect.runPromise(actions.apply());
    expect(tags).toEqual([FLEET.setup_etag]);
    await Effect.runPromise(Effect.flip(actions.apply()));
  });
});

test("a durable assistant failure is shown with its recovery action and remains unsuccessful", async () => {
  await withApi((request) => Response.json(new URL(request.url).pathname.endsWith("/messages")
    ? {items: [{...MESSAGE, state: MESSAGE_STATE.failed, failure: {
      reason: "model_unavailable", error_code: "UZ-AUTHOR-004", recovery_action: "select_model_then_send",
    }}], next_cursor: null} : {...DRAFT, accepted: ACCEPTED}), async (transport) => {
    const {output, rendered} = capture();
    const actions = new ChatActions(new DraftSession(transport, MODEL, {kind: DRAFT_SOURCE.blank}),
      new SetupSession(transport), output, () => Effect.void);
    await Effect.runPromise(actions.message(PRIVATE_TEXT));
    expect(actions.partial).toBe(true);
    expect(rendered.join("\n")).toContain("select_model_then_send");
  });
});

test("publication is separate from Save and never creates a runtime instance", async () => {
  const posts: string[] = [];
  await withApi((request) => {
    const path = new URL(request.url).pathname;
    if (request.method === "POST") { posts.push(path); return Response.json(PUBLICATION); }
    return Response.json(DRAFT);
  }, async (transport) => {
    const actions = new ChatActions(new DraftSession(transport, MODEL, {kind: DRAFT_SOURCE.blank}, DRAFT_ID),
      new SetupSession(transport), capture().output, () => Effect.void);
    await Effect.runPromise(actions.review());
    await Effect.runPromise(actions.publish("Argo"));
    expect(posts).toHaveLength(1);
    expect(posts[0]).toEndWith("/fleet-libraries");
  });
});
