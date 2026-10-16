// @vitest-environment node
import type { Draft } from "@/lib/api/draft-types";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { SetupUnavailable } from "@/lib/api/draft-request";
import { DraftClient, type PlatformPublisher } from "@/lib/api/drafts";
import { DRAFT_STATE, MESSAGE_STATE } from "@/lib/api/draft-types";
import { DRAFT_EVENT } from "@/lib/api/draft-events";
import { FakeEventSource } from "./helpers/fake-event-source";
import { ACCEPTED, APP_ORIGIN, DRAFT, DRAFT_ID, FLEET, FLEET_ID, MESSAGE, MODEL, MODEL_REF, PUBLICATION, REVIEW, WORKSPACE, response } from "./helpers/draft-fixtures";

type Call = {readonly path: string; readonly method: string; readonly body: unknown; readonly headers: Headers};
type Override = (call: Call) => Response | undefined | Promise<Response | undefined>;
const client = (publisher?: PlatformPublisher): DraftClient => new DraftClient({account: "owner-account", workspace: WORKSPACE, subject: "creator"}, publisher);
const clients: DraftClient[] = [];
const calls: Call[] = [];
let override: Override = () => undefined;
let current: Draft = DRAFT;

beforeEach(() => {
  calls.length = 0; current = structuredClone(DRAFT); override = () => undefined;
  FakeEventSource.instances = []; vi.stubGlobal("EventSource", FakeEventSource);
  vi.stubGlobal("fetch", vi.fn(async (input: string | URL | Request, init?: RequestInit) => {
    const path = new URL(typeof input === "string" ? input : input instanceof URL ? input.href : input.url, APP_ORIGIN).pathname;
    const call = {path, method: init?.method ?? "GET", body: init?.body ? JSON.parse(typeof init.body === "string" ? init.body : "null") : undefined, headers: new Headers(init?.headers)};
    calls.push(call);
    const custom = await override(call); if (custom) return custom;
    if (path.endsWith("/models")) return response({items: [MODEL], next_cursor: null});
    if (path.endsWith("/drafts") && call.method === "GET") return response({items: [], next_cursor: null});
    if (path.endsWith("/drafts") && call.method === "POST") return response({...current, accepted: ACCEPTED}, 201);
    if (path.endsWith("/messages") && call.method === "GET") return response({items: [MESSAGE], next_cursor: null});
    if (path.endsWith("/messages") && call.method === "POST") return response(ACCEPTED, 202);
    if (path.endsWith(`/drafts/${DRAFT_ID}`)) return response(current);
    if (path.endsWith(`/fleets/${FLEET_ID}`)) return response(FLEET);
    if (path.endsWith("/fleet-libraries") && call.method === "POST") return response(PUBLICATION, 201);
    throw new Error(`Unexpected fixture request ${call.method} ${path}`);
  }));
});

describe("private setup editing and recovery boundaries", () => {
  const edit = () => ({definition: structuredClone(REVIEW.effective.definition), instanceName: REVIEW.effective.instance_name});

  it("changes the authoring model locally before creation and conditionally after creation", async () => {
    const instance = await ready(); await instance.selectModel("model:second");
    expect(instance.getSnapshot().model).toBe("model:second"); expect(mutations()).toEqual([]);
    await instance.resume(DRAFT_ID); await instance.selectModel("model:third");
    expect(mutations()[0]?.body).toEqual({authoring_model_ref: "model:third"});
    expect(mutations()[0]?.headers.get("If-Match")).toBe(DRAFT.etag);
    expect(instance.getSnapshot().model).toBe("model:third");
  });

  it("previews an inline definition without creating a draft", async () => {
    const instance = await ready();
    instance.fresh({kind: "definition", definition: REVIEW.effective.definition});
    await instance.review();
    expect(instance.getSnapshot().preview).toMatchObject({definition: REVIEW.effective.definition, revision: "inline"});
    expect(mutations()).toEqual([]);
  });

  it.each(["same", "rename", "definition"])("creates an editable blank source then applies a %s edit", async (kind) => {
    const instance = await ready(); const input = edit();
    if (kind === "rename") input.instanceName = "new-name";
    if (kind === "definition") input.definition.instructions_md = "New instructions";
    await instance.edit(input);
    expect(instance.getSnapshot().draft?.id).toBe(DRAFT_ID);
    expect(mutations()[0]?.body).toMatchObject({source: {kind: "definition", definition: input.definition}});
    expect(mutations()).toHaveLength(kind === "same" ? 1 : 2);
    if (kind !== "same") expect(mutations()[1]?.headers.get("If-Match")).toBe(DRAFT.etag);
  });

  it("preserves a selected source and refuses an uneditable creation without patching it", async () => {
    const instance = await ready();
    instance.fresh({kind: "fleet", fleet_id: FLEET_ID, setup_revision: "1"});
    current = {...DRAFT, candidate: undefined};
    await expect(instance.edit(edit())).rejects.toMatchObject({status: 409, message: "The selected source has no editable setup."});
    expect(mutations()).toHaveLength(1);
    expect(mutations()[0]?.body).toMatchObject({source: {kind: "fleet", fleet_id: FLEET_ID}});
  });

  it("patches an existing candidate and refuses a draft with no proposal", async () => {
    const instance = await ready(true); await instance.review();
    await instance.edit({...edit(), runtimeModel: MODEL_REF});
    expect(mutations()[0]?.body).toMatchObject({runtime_model_ref: MODEL_REF});
    expect(instance.getSnapshot().reviewed).toBeNull();
    current = {...DRAFT, candidate: undefined, review: undefined};
    await instance.retry();
    await expect(instance.edit(edit())).rejects.toMatchObject({status: 409});
    await expect(instance.review()).rejects.toMatchObject({status: 409});
    expect(mutations()).toHaveLength(1);
  });

  it.each(["", "x".repeat(32_769), "🌱".repeat(8_193)])("rejects invalid message input before creating an operation (%#)", async (text) => {
    const instance = await ready();
    await expect(instance.send(text)).rejects.toMatchObject({status: 400});
    expect(instance.getSnapshot()).toMatchObject({pending: false, busy: false, error: "Messages must contain 1 to 32,768 bytes of text."});
    expect(mutations()).toEqual([]);
  });

  it("requires an authoring model and an existing conversation for applicable actions", async () => {
    override = ({path}) => path.endsWith("/models") ? response({items: [], next_cursor: null}) : undefined;
    const instance = await ready();
    await expect(instance.edit(edit())).rejects.toMatchObject({status: 400});
    await expect(instance.send("hello")).rejects.toMatchObject({status: 400});
    await expect(instance.stop()).rejects.toMatchObject({status: 409});
    await expect(instance.retry()).rejects.toMatchObject({status: 409});
    expect(mutations()).toEqual([]);
  });

  it("refuses Apply without a linked fleet and append publication without a prior publication", async () => {
    const instance = await ready(true); await instance.review();
    await expect(instance.apply()).rejects.toMatchObject({status: 409});
    await instance.review();
    await expect(instance.publish("recipe", true)).rejects.toMatchObject({status: 409});
    expect(mutations()).toEqual([]);
  });

  it("publishes a new entry and clears a deleted conversation", async () => {
    const instance = await ready(true); await instance.review(); await instance.publish("recipe", false);
    expect(instance.getSnapshot().notice).toContain(PUBLICATION.revision_id);
    expect(mutations()[0]?.body).toEqual({draft_candidate_id: DRAFT.candidate?.id, label: "recipe"});
    override = ({method}) => method === "DELETE" ? response(null, 204) : undefined;
    await instance.remove();
    expect(instance.getSnapshot()).toMatchObject({draft: null, messages: [], reviewed: null});
  });

  it("retains its operation and rejects overlapping edits until the request settles", async () => {
    const pending = Promise.withResolvers<Response>(); const instance = await ready();
    override = ({method}) => method === "POST" ? pending.promise : undefined;
    const sending = instance.send("hello");
    await expect(instance.resume(DRAFT_ID)).rejects.toMatchObject({status: 409});
    await expect(instance.review()).rejects.toMatchObject({status: 409});
    expect(() => instance.fresh()).toThrow("still in progress");
    pending.resolve(response({...DRAFT, accepted: ACCEPTED}, 201)); await sending;
    expect(instance.getSnapshot()).toMatchObject({draft: DRAFT, busy: false, pending: false});
    expect(mutations()).toHaveLength(1);
  });

  it("cannot restore private state through a late answer after disposal", async () => {
    const pending = Promise.withResolvers<Response>(); const instance = await ready(true);
    override = ({path}) => path.endsWith("/messages") ? pending.promise : undefined;
    const reading = instance.retry(); instance.dispose();
    pending.resolve(response({items: [MESSAGE], next_cursor: null}));
    await expect(reading).rejects.toMatchObject({status: 502});
    expect(instance.getSnapshot()).toMatchObject({draft: null, messages: [], busy: false});
  });

  it("locks further actions after access is revoked", async () => {
    const instance = await ready(true);
    override = () => response({}, 403);
    await expect(instance.retry()).rejects.toMatchObject({status: 403});
    const count = calls.length;
    await expect(instance.send("hello")).rejects.toMatchObject({status: 403});
    expect(calls).toHaveLength(count); expect(instance.getSnapshot().inaccessible).toBe(true);
  });
});
afterEach(() => {for (const instance of clients.splice(0)) instance.dispose(); vi.unstubAllGlobals(); vi.restoreAllMocks();});

async function ready(resume = false, publisher?: PlatformPublisher): Promise<DraftClient> {
  const instance = client(publisher); clients.push(instance); await instance.initialize();
  if (resume) await instance.resume(DRAFT_ID);
  return instance;
}
const mutations = (): Call[] => calls.filter((call) => call.method !== "GET");

describe("private browser chat", () => {
  it("reconciles a completed turn from one private stream and closes it", async () => {
    let running = true;
    override = ({path, method}) => path.endsWith("/messages") && method === "GET"
      ? response({items: [{...MESSAGE, state: running ? MESSAGE_STATE.running : MESSAGE_STATE.completed}], next_cursor: null}) : undefined;
    const instance = await ready(true);
    const source = FakeEventSource.instances[0];
    expect(source).toBeDefined(); expect(FakeEventSource.instances).toHaveLength(1);
    source?.open(); await vi.waitFor(() => expect(calls.filter((call) => call.path.endsWith("/messages"))).toHaveLength(2));
    running = false; source?.emitNamed(DRAFT_EVENT.completed, JSON.stringify({draft_id: DRAFT_ID}));
    await vi.waitFor(() => expect(instance.getSnapshot().messages[0]?.state).toBe(MESSAGE_STATE.completed));
    expect(source?.closed).toBe(true); expect(FakeEventSource.instances).toHaveLength(1);
  });
  it("opening reads choices and private draft cards without creating a resource", async () => {
    const instance = await ready();
    expect(instance.getSnapshot().model).toBe(MODEL_REF); expect(instance.getSnapshot().draft).toBeNull();
    expect(mutations()).toEqual([]);
  });
  it("distinguishes an unavailable setup service from a missing private chat", async () => {
    override = ({path}) => path.endsWith("/drafts") ? response({}, 404) : undefined;
    const opening = client(); clients.push(opening);
    await expect(opening.initialize()).rejects.toMatchObject({status: 404});
    expect(opening.getSnapshot()).toMatchObject({inaccessible: true, models: [], cards: [],
      error: "Fleet setup isn’t available in this workspace right now. Try again shortly, or return to Your fleets."});
    override = () => undefined;
    const resumed = await ready();
    override = ({path}) => path.endsWith(`/drafts/${DRAFT_ID}`) ? response({}, 404) : undefined;
    await expect(resumed.resume(DRAFT_ID)).rejects.toMatchObject({status: 404});
    expect(resumed.getSnapshot()).toMatchObject({inaccessible: true, messages: [],
      error: "This chat is no longer available. It may have been removed, or your access may have changed."});
    expect(mutations()).toEqual([]);
  });
  it("ordinary first chat has one atomic identity and never saves or publishes", async () => {
    const instance = await ready(); await instance.send("Investigate failures");
    const sent = mutations(); expect(sent).toHaveLength(1);
    expect(sent[0]?.body).toMatchObject({source: {kind: "blank"}, authoring_model_ref: MODEL_REF, message: "Investigate failures"});
    expect(sent[0]?.headers.get("Idempotency-Key")).toBeNull(); expect(instance.getSnapshot().draft?.id).toBe(DRAFT_ID);
    expect(instance.getSnapshot().messages[0]?.text).toBe(MESSAGE.text);
  });
  it("retries a lost creation answer with its original body key", async () => {
    let attempted = false;
    override = ({path, method}) => {
      if (path.endsWith("/drafts") && method === "POST" && !attempted) {attempted = true; return response({}, 503);}
      return undefined;
    };
    const instance = await ready(); await expect(instance.send("Investigate failures")).rejects.toMatchObject({status: 503});
    expect(instance.getSnapshot().pending).toBe(true);
    expect(() => instance.fresh()).toThrow("current operation");
    await instance.retry();
    expect(mutations()).toHaveLength(2); expect(mutations()[1]?.body).toEqual(mutations()[0]?.body);
    expect(instance.getSnapshot().pending).toBe(false);
  });
  it.each([401, 403, 404, 410])("clears messages, candidate, review and operation keys on access loss %i", async (status) => {
    const instance = await ready(true); await instance.review();
    override = ({path, method}) => path.endsWith("/messages") && method === "POST" ? new Response("unavailable", {status}) : undefined;
    await expect(instance.send("Private follow-up")).rejects.toMatchObject({status});
    expect(instance.getSnapshot()).toMatchObject({draft: null, messages: [], models: [], cards: [], reviewed: null, fleet: null, pending: false, inaccessible: true});
  });
  it("requires a displayed Review and replays partial Save with the exact header key", async () => {
    const instance = await ready(true);
    await expect(instance.save()).rejects.toMatchObject({status: 409});
    await instance.review();
    override = ({path, method}) => path.endsWith("/fleets") && method === "POST" ? response(FLEET, 201) : undefined;
    await instance.save(); expect(instance.getSnapshot().fleet?.id).toBe(FLEET_ID);
    await instance.retry();
    const saved = mutations(); expect(saved).toHaveLength(2); expect(saved[0]?.body).toEqual(saved[1]?.body);
    expect(saved[0]?.headers.get("Idempotency-Key")).toBe(saved[1]?.headers.get("Idempotency-Key"));
    expect(saved[0]?.body).toMatchObject({review_digest: REVIEW.review_digest, start: {kind: "none"}});
  });
  it("pins Apply to the setup tag whose revision matches the displayed Review", async () => {
    current = {...DRAFT, linked_fleet_id: FLEET_ID, review: {...REVIEW, effective: {...REVIEW.effective, target_setup_revision: "1"}}};
    const instance = await ready(true); await instance.review();
    override = ({path, method}) => path.endsWith(`/fleets/${FLEET_ID}/revisions`) && method === "POST" ? response({fleet_id: FLEET_ID, setup_revision: "2", setup_etag: '"two"', etag: '"resource-two"'}, 201) : undefined;
    await instance.apply();
    expect(mutations()[0]?.headers.get("If-Match")).toBe(FLEET.setup_etag);
    expect(mutations()[0]?.body).toMatchObject({review_digest: REVIEW.review_digest});
    expect(instance.getSnapshot().reviewed).toBeNull();
  });
  it("refuses Improve when its pinned fleet setup changed before Review", async () => {
    current = {...DRAFT, linked_fleet_id: FLEET_ID, review: {...REVIEW, effective: {...REVIEW.effective, target_setup_revision: "2"}}};
    const instance = await ready(true);
    await expect(instance.review()).rejects.toMatchObject({status: 409}); expect(mutations()).toEqual([]);
  });
  it("retains a publication revision tag after a lost answer", async () => {
    current = {...DRAFT, publication: {library_id: PUBLICATION.library_id, revision_id: PUBLICATION.revision_id, canonical_digest: "prior"}};
    let attempted = false;
    override = ({path, method}) => {
      if (path.endsWith(`/fleet-libraries/${PUBLICATION.library_id}`)) return response({etag: '"original-library"'});
      if (path.endsWith("/revisions") && method === "POST") {
        if (!attempted) {attempted = true; return response({}, 503);} return response(PUBLICATION, 201);
      }
      return undefined;
    };
    const instance = await ready(true); await instance.review();
    await expect(instance.publish("Recipe", true)).rejects.toMatchObject({status: 503}); await instance.retry();
    expect(mutations()[1]?.headers.get("If-Match")).toBe('"original-library"');
    expect(mutations()[1]?.headers.get("Idempotency-Key")).toBe(mutations()[0]?.headers.get("Idempotency-Key"));
    expect(calls.filter((call) => call.path.endsWith(`/fleet-libraries/${PUBLICATION.library_id}`))).toHaveLength(1);
  });
  it("reloads a retained fleet and resumes it with its fresh resource tag", async () => {
    current = {...DRAFT, state: DRAFT_STATE.saving, linked_fleet_id: FLEET_ID};
    const instance = await ready(true);
    expect(instance.getSnapshot().fleet?.id).toBe(FLEET_ID); expect(mutations()).toEqual([]);
    await instance.retry();
    expect(mutations()).toHaveLength(1); expect(mutations()[0]?.method).toBe("PATCH");
    expect(mutations()[0]?.headers.get("If-Match")).toBe(FLEET.etag);
    expect(mutations()[0]?.body).toEqual({status: "active"});
  });
  it("clears private data and cancels future I/O on disposal", async () => {
    const instance = await ready(true); instance.dispose(); const before = calls.length;
    expect(instance.getSnapshot().messages).toEqual([]); expect(instance.getSnapshot().draft).toBeNull();
    await expect(instance.send("must not send")).rejects.toMatchObject({status: 400}); expect(calls).toHaveLength(before);
  });
  it("Stop uses the observed durable watermark and reconciles the terminal result", async () => {
    const instance = await ready(true);
    override = ({path}) => path.endsWith("/cancellations") ? response({cancelled_through: MESSAGE.sequence, stopped: [{sequence: MESSAGE.sequence, state: MESSAGE_STATE.cancelled}]}) : undefined;
    await instance.stop(); expect(mutations()[0]?.body).toEqual({through_sequence: MESSAGE.sequence});
  });
  it("stages a reviewed private candidate with one retained platform operation after a lost answer", async () => {
    const published: {body: unknown; key: string}[] = [];
    let lost = true;
    const instance = await ready(true, async (body, key) => {
      published.push({body, key});
      if (lost) { lost = false; throw new SetupUnavailable("Answer lost", 503); }
      return {...PUBLICATION, library_id: "native-staged"};
    });
    await instance.review();
    await expect(instance.stagePlatform("Reviewed recipe")).rejects.toMatchObject({status: 503});
    expect(instance.getSnapshot().pending).toBe(true);
    await expect(instance.publish("Changed input", false)).rejects.toMatchObject({status: 409});
    await instance.retry();
    expect(published).toHaveLength(2); expect(published[1]).toEqual(published[0]);
    expect(published[0]?.body).toEqual({workspace_id: WORKSPACE, draft_candidate_id: DRAFT.candidate?.id, label: "Reviewed recipe"});
    expect(instance.getSnapshot().notice).toContain("Publish it explicitly");
    expect(instance.getSnapshot().pending).toBe(false); expect(mutations()).toEqual([]);
  });
  it("never stages without a fresh Review", async () => {
    const publisher = vi.fn();
    const instance = await ready(true, publisher);
    await expect(instance.stagePlatform("Recipe")).rejects.toMatchObject({status: 409});
    expect(publisher).not.toHaveBeenCalled(); expect(instance.getSnapshot().pending).toBe(false);
  });
  it("hides staging without operator capability and clears private data on a scoped refusal", async () => {
    const instance = await ready(true);
    expect(instance.canStagePlatform).toBe(false);
    await instance.review();
    await expect(instance.stagePlatform("Recipe")).rejects.toMatchObject({status: 403});
    expect(instance.getSnapshot()).toMatchObject({draft: null, messages: [], reviewed: null, inaccessible: true});
  });

  it.each([false, true])("closes a deleted draft stream without restoring data (disposed: %s)", async (disposed) => {
    override = ({path}) => path.endsWith("/messages")
      ? response({items: [{...MESSAGE, state: MESSAGE_STATE.running}], next_cursor: null}) : undefined;
    const instance = await ready(true);
    const source = FakeEventSource.instances[0];
    source?.emitNamed(DRAFT_EVENT.deleted, JSON.stringify({draft_id: DRAFT_ID}));
    if (disposed) instance.dispose();
    await vi.waitFor(() => expect(instance.getSnapshot().draft).toBeNull());
    expect(instance.getSnapshot().inaccessible).toBe(!disposed);
    expect(instance.getSnapshot().messages).toEqual([]);
    expect(source?.closed).toBe(true);
  });

  it("offers a durable refresh when stream following reaches its time limit", async () => {
    const limit = new AbortController(); const timeout = AbortSignal.timeout.bind(AbortSignal);
    vi.spyOn(AbortSignal, "timeout").mockImplementation((milliseconds) => milliseconds === 330_000 ? limit.signal : timeout(milliseconds));
    override = ({path}) => path.endsWith("/messages")
      ? response({items: [{...MESSAGE, state: MESSAGE_STATE.running}], next_cursor: null}) : undefined;
    const instance = await ready(true);
    limit.abort();
    await vi.waitFor(() => expect(instance.getSnapshot().notice).toContain("Work is still pending"));
    expect(instance.getSnapshot().messages[0]?.state).toBe(MESSAGE_STATE.running);
    expect(FakeEventSource.instances[0]?.closed).toBe(true);
  });

});
