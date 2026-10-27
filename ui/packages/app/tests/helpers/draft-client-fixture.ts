// @vitest-environment node
import type { Draft, Fleet } from "@/lib/api/draft-types";
import { vi } from "vitest";
import { DraftClient, type PlatformPublisher } from "@/lib/api/drafts";
import { FakeEventSource } from "./fake-event-source";
import { ACCEPTED, APP_ORIGIN, DRAFT, DRAFT_ID, FLEET, FLEET_ID, MESSAGE, MODEL, PUBLICATION, WORKSPACE, response } from "./draft-fixtures";

type Call = {readonly path: string; readonly search: string; readonly method: string; readonly body: unknown; readonly headers: Headers};
type Override = (call: Call) => Response | undefined | Promise<Response | undefined>;

export class DraftClientFixture {
  readonly calls: Call[] = [];
  readonly clients: DraftClient[] = [];
  current: Draft = DRAFT;
  fleet: Fleet = FLEET;
  override: Override = () => undefined;
  mount(): void {
    this.calls.length = 0; this.current = structuredClone(DRAFT); this.fleet = structuredClone(FLEET); this.override = () => undefined;
    FakeEventSource.instances = []; vi.stubGlobal("EventSource", FakeEventSource);
    vi.stubGlobal("fetch", vi.fn(async (input: string | URL | Request, init?: RequestInit) => {
      const url = new URL(typeof input === "string" ? input : input instanceof URL ? input.href : input.url, APP_ORIGIN);
      const path = url.pathname;
      const call = {path, search: url.search, method: init?.method ?? "GET", body: init?.body ? JSON.parse(typeof init.body === "string" ? init.body : "null") : undefined, headers: new Headers(init?.headers)};
      this.calls.push(call);
      const custom = await this.override(call); if (custom) return custom;
      if (path.endsWith("/models")) return response({items: [MODEL], next_cursor: null});
      if (path.endsWith("/drafts") && call.method === "GET") return response({items: [], next_cursor: null});
      if (path.endsWith("/drafts") && call.method === "POST") return response({...this.current, accepted: ACCEPTED}, 201);
      if (path.includes("/fleets/") && path.endsWith("/messages") && call.method === "GET") return response({items: [], next_cursor: null});
      if (path.endsWith("/messages") && call.method === "GET") return response({items: [MESSAGE], next_cursor: null});
      if (path.endsWith("/messages") && call.method === "POST") return response(ACCEPTED, 202);
      if (path.endsWith(`/drafts/${DRAFT_ID}`)) return response(this.current);
      if (path.endsWith("/fleets") && call.method === "POST") return response(FLEET, 201);
      if (path.endsWith(`/fleets/${FLEET_ID}`)) return response(this.fleet);
      if (path.endsWith("/fleet-libraries") && call.method === "POST") return response(PUBLICATION, 201);
      throw new Error(`Unexpected fixture request ${call.method} ${path}`);
    }));
  }
  create(publisher?: PlatformPublisher): DraftClient {
    const client = new DraftClient({account: "owner-account", workspace: WORKSPACE, subject: "creator"}, publisher);
    this.clients.push(client); return client;
  }
  readonly ready = async (resume = false, publisher?: PlatformPublisher): Promise<DraftClient> => {
    const client = this.create(publisher); await client.initialize();
    if (resume) await client.resume(DRAFT_ID);
    return client;
  };
  readonly mutations = (): Call[] => this.calls.filter((call) => call.method !== "GET");
  dispose(): void {
    for (const instance of this.clients.splice(0)) instance.dispose();
    vi.unstubAllGlobals(); vi.restoreAllMocks();
  }
}
