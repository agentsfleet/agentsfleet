// @vitest-environment node
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { DraftClient } from "@/lib/api/drafts";
import { FakeEventSource } from "./helpers/fake-event-source";
import { APP_ORIGIN, DRAFT, DRAFT_ID, MESSAGE, MODEL, WORKSPACE, response } from "./helpers/draft-fixtures";

const savedModel = "model:beyond-loaded-pages";
const clients: DraftClient[] = [];
const methods: string[] = [];

beforeEach(() => {
  methods.length = 0;
  FakeEventSource.instances = [];
  vi.stubGlobal("EventSource", FakeEventSource);
  vi.stubGlobal("fetch", vi.fn(async (input: string, init?: RequestInit) => {
    methods.push(init?.method ?? "GET");
    const path = new URL(input, APP_ORIGIN).pathname;
    if (path.endsWith("/models")) return response({items: [MODEL], next_cursor: "more-models"});
    if (path.endsWith("/drafts")) return response({items: [], next_cursor: "more-chats"});
    if (path.endsWith("/messages")) return response({items: [MESSAGE], next_cursor: null});
    if (path.endsWith(`/drafts/${DRAFT_ID}`)) return response({...DRAFT, authoring_model_ref: savedModel});
    throw new Error(`Unexpected fixture request ${path}`);
  }));
});

afterEach(() => {
  clients.splice(0).forEach(client => client.dispose());
  vi.unstubAllGlobals();
});

it.each(["models", "chats", "refresh"] as const)("preserves the persisted model during %s pagination or refresh", async (list) => {
  const client = new DraftClient({account: "owner-account", workspace: WORKSPACE, subject: "creator"});
  clients.push(client);
  await client.initialize();
  await client.resume(DRAFT_ID);
  expect(client.getSnapshot().model).toBe(savedModel);
  if (list === "refresh") await client.initialize(); else await client.loadMore(list);
  expect(client.getSnapshot()).toMatchObject({model: savedModel, draft: {authoring_model_ref: savedModel}});
  expect(methods.every(method => method === "GET")).toBe(true);
});
