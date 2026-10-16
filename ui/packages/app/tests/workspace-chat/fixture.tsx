import type { Draft, Message } from "@/lib/api/draft-types";
import { vi } from "vitest";
import { DRAFT, FLEET, FLEET_ID, MESSAGE, MODEL, PUBLICATION, WORKSPACE, response } from "@/tests/helpers/draft-fixtures";
import { FakeEventSource } from "@/tests/helpers/fake-event-source";
// Load the real dynamic children before interaction deadlines start.
import "@/components/domain/chat/DraftConversation";
import "@/components/domain/chat/ReviewSetup";

export type Call = {path: string; method: string; body: unknown; headers: Headers};
const auth = vi.hoisted(() => ({isLoaded: true, isSignedIn: true, userId: "creator"}));
vi.mock("@/lib/auth/client", () => ({useCurrentUser: () => auth}));
vi.mock("@/app/(dashboard)/admin/fleet-libraries/actions", () => ({stagePlatformLibraryAction: vi.fn()}));

export class ChatFixture {
  readonly auth = auth;
  readonly calls: Call[] = [];
  draft: Draft = structuredClone(DRAFT);
  messages: Message[] = [MESSAGE, {...MESSAGE, role: "assistant", text: "Your setup is ready."}];
  saveAttempts = 0;
  failSave = false;
  readonly fetch = vi.fn(async (input: string | URL | Request, init?: RequestInit): Promise<Response> => {
    const path = new URL(typeof input === "string" ? input : input instanceof URL ? input.href : input.url, "http://app.test").pathname;
    const body: unknown = typeof init?.body === "string" ? JSON.parse(init.body) : undefined;
    const call = {path, method: init?.method ?? "GET", body, headers: new Headers(init?.headers)};
    this.calls.push(call);
    if (path.endsWith("/models")) return response({items: [MODEL], next_cursor: null});
    if (path.endsWith("/drafts") && call.method === "GET") return response({items: [], next_cursor: null});
    if (path.endsWith("/drafts") && call.method === "POST") return response({...this.draft, accepted: {status: "accepted", sequence: "1", replayed: false}}, 201);
    if (path.endsWith("/messages")) return response({items: this.messages, next_cursor: null});
    if (path.endsWith(`/drafts/${this.draft.id}`)) return response(this.draft);
    if (path.endsWith("/fleets") && call.method === "POST") {
      this.saveAttempts++;
      if (this.failSave && this.saveAttempts === 1) throw new TypeError("network interrupted");
      return response({...FLEET, status: "active", setup: {state: "ready", checks: []}}, 201);
    }
    if (path.endsWith(`/fleets/${FLEET_ID}`)) return response({...FLEET, status: "active", setup: {state: "ready", checks: []}});
    if (path.endsWith("/fleet-libraries") && call.method === "POST") return response(PUBLICATION, 201);
    throw new Error(`Unexpected chat fixture request ${call.method} ${path}`);
  });
  mount(): void {
    auth.isLoaded = true; auth.isSignedIn = true; auth.userId = "creator";
    FakeEventSource.instances = [];
    vi.stubGlobal("EventSource", FakeEventSource);
    vi.stubGlobal("fetch", this.fetch);
  }
  writes(): Call[] { return this.calls.filter((call) => call.method !== "GET"); }
}

export const CHAT_PROPS = {account: "account-one", workspace: WORKSPACE};
