import type { ReactNode } from "react";
import { StrictMode } from "react";
import type { NativeFleetDetail } from "@/lib/api/fleet-detail";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { z } from "zod";
import { TooltipProvider } from "@agentsfleet/design-system";
import { FleetDetailSchema } from "@/lib/api/fleet-detail";
import { FLEET_DETAIL, FLEET_ID, GRANT_ID, MODEL, SCHEDULE, TRIGGER_ID, WORKSPACE, response } from "@/tests/helpers/fleet-fixtures";
import { FleetWorkspace } from "./FleetWorkspace";
import { FleetListSections } from "./FleetListSections";
import { FleetListsProvider, useFleetLists, useReloadListsAfter } from "./FleetListsProvider";
import { FleetNetworkSection } from "../network/FleetNetworkSection";
import { FleetModelSelect } from "../models/FleetModelSelect";

const identity = vi.hoisted(() => ({userId: "person" as string | null, isLoaded: true}));
const router = vi.hoisted(() => ({refresh: vi.fn()}));
vi.mock("next/navigation", () => ({useRouter: () => router}));
vi.mock("@/lib/auth/client", () => ({useCurrentUser: () => identity}));

const BASE = `/live/v1/workspaces/${WORKSPACE}/fleets/${FLEET_ID}`;
const WEBHOOK = {id: TRIGGER_ID, kind: "webhook", source: "sentry", url: "https://api.example.test/v1/webhooks/fleet/sentry", created_at: "2026-11-24T10:00:00Z"};
const APP_EVENT = {id: "0199a801-1000-7000-8000-0000000000c1", kind: "app_event", source: "github", repositories: ["org/app"], events: ["check_suite"], created_at: "2026-11-24T10:00:00Z"};
const APP_EVENT_ANY = {...APP_EVENT, id: "0199a801-1000-7000-8000-0000000000c2", repositories: null, events: null};
const MENTION = {id: "0199a801-1000-7000-8000-0000000000c3", kind: "mention", source: "slack", channel: "C42", created_at: "2026-11-24T10:00:00Z"};
const MENTION_ANY = {...MENTION, id: "0199a801-1000-7000-8000-0000000000c4", channel: null};
const FUTURE = {id: "0199a801-1000-7000-8000-0000000000c5", kind: "email", source: "inbox", created_at: "2026-11-24T10:00:00Z"};
const GITHUB = {id: GRANT_ID, kind: "integration", name: "github", status: "approved", scope: {repositories: ["org/app", "org/web"], access: "write"}};
const SENTRY = {id: "0199a801-1000-7000-8000-0000000000d1", kind: "secret", name: "sentry", status: "approved"};
const READ_ONLY = {...GITHUB, kind: undefined, id: "0199a801-1000-7000-8000-0000000000d2", name: "github", scope: {repositories: ["org/docs"], access: "read"}};
const PENDING = {id: "0199a801-1000-7000-8000-0000000000d3", kind: "secret", name: "grafana", status: "pending"};
const PIN = {key: "evidence", content: "Always link the evidence.", category: "core", updated_at: "2026-11-24T10:00:00Z", visibility: "fleet", writer_fleet_id: FLEET_ID};
type Route = (init: RequestInit) => Response | Promise<Response>;
let routes: Map<string, Route>;
let calls: {method: string; path: string; body?: unknown; headers?: HeadersInit}[];
let detail: NativeFleetDetail;

function serve(method: string, path: string, route: Route) {routes.set(`${method} ${path}`, route);}
function lists({schedules = [SCHEDULE], triggers = [WEBHOOK], grants = [GITHUB, PENDING], pinned = [PIN]}: {schedules?: unknown[]; triggers?: unknown[]; grants?: unknown[]; pinned?: unknown[]} = {}) {
  detail = FleetDetailSchema.parse({...FLEET_DETAIL, schedules, triggers, grants});
  serve("GET", `${BASE}/schedules`, () => response({schedules}));
  serve("GET", `${BASE}/triggers`, () => response({items: triggers}));
  serve("GET", `${BASE}/integration-grants`, () => response({items: grants}));
  serve("GET", `${BASE}/memories?limit=100`, () => response({items: pinned, total: pinned.length, next_cursor: null}));
}
function page(children: ReactNode = <><FleetListSections /><FleetNetworkSection /><FleetModelSelect label="Saved model" /></>, fleet = detail) {
  return render(<TooltipProvider><FleetWorkspace account="account" workspace={WORKSPACE} fleet={fleet} models={[MODEL, {...MODEL, ref: "model:other", label: "Other model"}]} viewer="person">{children}</FleetWorkspace></TooltipProvider>);
}
const section = (name: string) => screen.getByRole("region", {name});

beforeEach(() => {
  routes = new Map(); calls = []; detail = FleetDetailSchema.parse(FLEET_DETAIL); identity.userId = "person"; identity.isLoaded = true; router.refresh.mockClear();
  vi.stubGlobal("fetch", vi.fn(async (path: string, init: RequestInit = {}) => {
    const method = init.method ?? "GET";
    calls.push({method, path, body: typeof init.body === "string" ? JSON.parse(init.body) : undefined, headers: init.headers});
    const route = routes.get(`${method} ${path}`);
    return route ? route(init) : response({user_message: `unrouted ${method} ${path}`}, 599);
  }));
});
afterEach(() => {cleanup(); vi.unstubAllGlobals();});

describe("the fleet page's lists", () => {
  it("renders the fleet detail's three lists immediately and reads only pinned memory", async () => {
    lists();
    page();
    expect(screen.getByText(SCHEDULE.description)).toBeTruthy();
    expect(screen.getByText("Webhook · sentry")).toBeTruthy();
    expect(screen.getByRole("button", {name: "Revoke access: github"})).toBeTruthy();
    await screen.findByText("Always link the evidence.");
    expect(calls.map(call => `${call.method} ${call.path}`)).toEqual([`GET ${BASE}/memories?limit=100`]);
  });

  it("lists what wakes the fleet, what it can access and what it remembers, each with Remove", async () => {
    lists({triggers: [WEBHOOK, APP_EVENT, APP_EVENT_ANY, MENTION, MENTION_ANY, FUTURE], grants: [GITHUB, READ_ONLY, SENTRY, PENDING],
      schedules: [SCHEDULE, {...SCHEDULE, schedule_id: "0199a801-1000-7000-8000-0000000000e1", description: "Every hour at :07", sync: "failed"}]});
    page();
    const wakes = await screen.findByRole("list", {name: "Wakes on"});
    expect(within(wakes).getAllByRole("listitem").map(row => row.textContent)).toEqual([
      SCHEDULE.description, "Every hour at :07Registration failed",
      `Webhook · sentry${WEBHOOK.url}`, "GitHub · org/appcheck_suite", "GitHub · every repository", "Slack · C42", "Slack · any channel", "email · inbox"]);
    const access = await screen.findByRole("list", {name: "Can access"});
    expect(within(access).getAllByRole("listitem").map(row => row.textContent)).toEqual([
      "githuborg/app, org/web · Read and write", "githuborg/docs · Read only", "sentry"]);
    expect(within(access).queryByText("grafana")).toBeNull();
    expect(within(await screen.findByRole("list", {name: "Remembers"})).getByText("Always link the evidence.")).toBeTruthy();
    expect(screen.getByRole("button", {name: `Delete schedule: ${SCHEDULE.description}`})).toBeTruthy();
    expect(screen.getByRole("button", {name: "Delete trigger: Webhook · sentry"})).toBeTruthy();
    expect(screen.getByRole("button", {name: "Revoke access: sentry"})).toBeTruthy();
    expect(screen.getByRole("button", {name: "Remove pinned memory: evidence"})).toBeTruthy();
  });

  it("revokes an approved web host independently of a credential with the same name", async () => {
    const name = "support.vendor.example";
    const host = {...GITHUB, kind: "host", name};
    lists({grants: [host, {...SENTRY, name}, {...PENDING, kind: "host", name: "denied.vendor.example", status: "revoked"}]});
    serve("DELETE", `${BASE}/integration-grants/${host.id}`, () => new Response(null, {status: 204}));
    page();
    const access = await screen.findByRole("list", {name: "Can access"});
    expect(within(access).getAllByText(name)).toHaveLength(2);
    expect(within(access).getByText("Web host for future runs")).toBeTruthy();
    expect(within(access).queryByText("denied.vendor.example")).toBeNull();
    fireEvent.click(within(access).getByRole("button", {name: `Revoke web access: ${name}`}));
    fireEvent.click(screen.getByRole("button", {name: "Revoke access"}));
    await waitFor(() => expect(within(access).getAllByText(name)).toHaveLength(1));
    expect(within(access).getByRole("button", {name: `Revoke access: ${name}`})).toBeTruthy();
  });

  it("says what each empty list means", async () => {
    lists({schedules: [], triggers: [], grants: [PENDING], pinned: []});
    page();
    expect(await screen.findByText("Only a message wakes this fleet.")).toBeTruthy();
    expect(await screen.findByText("No access approved yet.")).toBeTruthy();
    expect(await screen.findByText("Nothing pinned yet. Ask the fleet to remember something.")).toBeTruthy();
  });

  it("shows a list that could not be read with a retry, and reads all four again", async () => {
    lists();
    serve("GET", `${BASE}/triggers`, () => response({user_message: "Triggers are unavailable right now."}, 503));
    function Reload() {const lists = useFleetLists(); return <button onClick={lists?.reload}>Refresh</button>;}
    page(<><FleetListSections /><Reload /></>);
    fireEvent.click(screen.getByRole("button", {name: "Refresh"}));
    const wakes = section("Wakes on");
    expect(await within(wakes).findByText("Triggers are unavailable right now.")).toBeTruthy();
    expect(within(section("Can access")).getByRole("list")).toBeTruthy();
    serve("GET", `${BASE}/triggers`, () => response({items: [WEBHOOK]}));
    serve("GET", `${BASE}/schedules`, () => response({user_message: "Schedules are unavailable right now."}, 503));
    fireEvent.click(within(wakes).getByRole("button", {name: "Try again"}));
    expect(await within(wakes).findByText("Schedules are unavailable right now.")).toBeTruthy();
    expect(calls.filter(call => call.path === `${BASE}/integration-grants`)).toHaveLength(2);
  });

  it("removing a pinned memory deletes it", async () => {
    lists();
    serve("DELETE", `${BASE}/memories/evidence`, () => response(null, 204));
    page();
    fireEvent.click(await screen.findByRole("button", {name: "Remove pinned memory: evidence"}));
    const dialog = await screen.findByRole("alertdialog");
    expect(calls.some(call => call.method === "DELETE")).toBe(false);
    fireEvent.click(within(dialog).getByRole("button", {name: "Remove"}));
    await waitFor(() => expect(screen.queryByText("Always link the evidence.")).toBeNull());
    expect(calls.filter(call => call.method === "DELETE").map(call => call.path)).toEqual([`${BASE}/memories/evidence`]);
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(await screen.findByText("Nothing pinned yet. Ask the fleet to remember something.")).toBeTruthy();
  });

  it("removes a trigger and an approved grant with one delete each", async () => {
    lists();
    serve("DELETE", `${BASE}/triggers/${TRIGGER_ID}`, () => response(null, 204));
    serve("DELETE", `${BASE}/integration-grants/${GRANT_ID}`, () => response(null, 204));
    page();
    fireEvent.click(await screen.findByRole("button", {name: "Delete trigger: Webhook · sentry"}));
    fireEvent.click(within(await screen.findByRole("alertdialog")).getByRole("button", {name: "Delete trigger"}));
    await waitFor(() => expect(screen.queryByText("Webhook · sentry")).toBeNull());
    fireEvent.click(screen.getByRole("button", {name: "Revoke access: github"}));
    fireEvent.click(within(await screen.findByRole("alertdialog")).getByRole("button", {name: "Revoke access"}));
    await waitFor(() => expect(screen.queryByRole("button", {name: "Revoke access: github"})).toBeNull());
    expect(calls.filter(call => call.method === "DELETE").map(call => call.path)).toEqual([`${BASE}/triggers/${TRIGGER_ID}`, `${BASE}/integration-grants/${GRANT_ID}`]);
  });

  it.each([
    [response({...SCHEDULE, status: "deleting", sync: "syncing"}), "Removal requested"],
    [response(null, 204), null],
  ] as const)("keeps a schedule the scheduler has yet to drop, and drops one it confirmed", async (answer, left) => {
    lists();
    serve("DELETE", `${BASE}/schedules/${SCHEDULE.schedule_id}`, () => answer.clone());
    page();
    fireEvent.click(await screen.findByRole("button", {name: `Delete schedule: ${SCHEDULE.description}`}));
    fireEvent.click(within(await screen.findByRole("alertdialog")).getByRole("button", {name: "Delete schedule"}));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    const wakes = screen.getByRole("list", {name: "Wakes on"});
    if (left) expect(within(wakes).getByText(left)).toBeTruthy();
    else expect(within(wakes).queryByText(SCHEDULE.description)).toBeNull();
  });

  it("retries a schedule the scheduler never registered, and says why a retry was refused", async () => {
    const failed = {...SCHEDULE, schedule_id: "0199a801-1000-7000-8000-0000000000e2", description: "Every hour at :07", sync: "failed", last_error: "The scheduler refused the expression."};
    lists({schedules: [SCHEDULE, failed]});
    serve("POST", `${BASE}/schedules/${failed.schedule_id}/sync`, () => response({user_message: "The scheduler is unavailable."}, 503));
    page();
    const wakes = await screen.findByRole("list", {name: "Wakes on"});
    const recovery = screen.getByLabelText("Schedule recovery");
    expect(within(recovery).getByText("Every hour at :07")).toBeTruthy();
    expect(within(recovery).getByText("The scheduler refused the expression.")).toBeTruthy();
    fireEvent.click(within(recovery).getByRole("button", {name: "Retry registration"}));
    expect(await within(section("Wakes on")).findByText("The scheduler is unavailable.")).toBeTruthy();
    serve("POST", `${BASE}/schedules/${failed.schedule_id}/sync`, () => response({...failed, sync: "synced", last_error: null}));
    fireEvent.click(within(recovery).getByRole("button", {name: "Retry registration"}));
    await waitFor(() => expect(within(wakes).queryByText("Registration failed")).toBeNull());
    expect(screen.queryByRole("button", {name: "Retry registration"})).toBeNull();
    expect(within(section("Wakes on")).queryByText("The scheduler is unavailable.")).toBeNull();
    expect(calls.filter(call => call.method === "POST").map(call => call.body)).toEqual([undefined, undefined]);
  });

  it("names one failed schedule's retry without repeating its name", async () => {
    lists({schedules: [{...SCHEDULE, sync: "syncing", last_error: null}]});
    page();
    const recovery = await screen.findByLabelText("Schedule recovery");
    expect(within(recovery).queryByText(SCHEDULE.description)).toBeNull();
    expect(within(recovery).getByRole("button", {name: "Retry registration"})).toBeTruthy();
  });

  it("keeps a row and its dialog when the removal is refused, and cancels without a write", async () => {
    lists();
    serve("DELETE", `${BASE}/integration-grants/${GRANT_ID}`, () => response({user_message: "Only a signed-in person can remove access."}, 403));
    page();
    fireEvent.click(await screen.findByRole("button", {name: "Revoke access: github"}));
    const dialog = await screen.findByRole("alertdialog");
    fireEvent.click(within(dialog).getByRole("button", {name: "Revoke access"}));
    expect(await within(dialog).findByText(/doesn’t have access here/)).toBeTruthy();
    expect(screen.getByRole("button", {name: "Revoke access: github", hidden: true})).toBeTruthy();
    fireEvent.click(within(dialog).getByRole("button", {name: "Cancel"}));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    fireEvent.click(screen.getByRole("button", {name: "Revoke access: github"}));
    expect(within(await screen.findByRole("alertdialog")).queryByText(/doesn’t have access here/)).toBeNull();
  });
});

describe("the lists' owner", () => {
  it("cancels a revoke and schedule retry when their viewer unmounts", async () => {
    lists();
    const remote = Promise.withResolvers<void>();
    const completed = Promise.withResolvers<(string | null)[]>();
    const signals: (AbortSignal | null | undefined)[] = [];
    serve("POST", `${BASE}/schedules/${SCHEDULE.schedule_id}/sync`, async init => {signals.push(init.signal); await remote.promise; return response(SCHEDULE);});
    serve("DELETE", `${BASE}/integration-grants/${GRANT_ID}`, async init => {signals.push(init.signal); await remote.promise; return response(null, 204);});
    function Actions() {
      const lists = useFleetLists();
      return <button onClick={() => {if (lists) completed.resolve(Promise.all([lists.retrySchedule(SCHEDULE), lists.remove({kind: "grant", grant: GITHUB})]));}}>Update</button>;
    }
    const view = render(<FleetListsProvider workspace={WORKSPACE} fleet={FLEET_ID} initial={detail}><Actions /></FleetListsProvider>);
    fireEvent.click(screen.getByRole("button", {name: "Update"}));
    expect(signals.map(signal => signal?.aborted)).toEqual([false, false]);
    view.unmount();
    expect(signals.map(signal => signal?.aborted)).toEqual([true, true]);
    remote.resolve();
    expect(await completed.promise).toEqual([expect.any(String), expect.any(String)]);
  });

  it("keeps a confirmed revoke and schedule retry when earlier GETs finish after both writes", async () => {
    const failed = {...SCHEDULE, sync: "failed", last_error: "Scheduler unavailable."};
    lists({schedules: [failed]});
    const writes = Promise.withResolvers<void>();
    const reads = Promise.withResolvers<void>();
    serve("POST", `${BASE}/schedules/${SCHEDULE.schedule_id}/sync`, async () => {await writes.promise; return response(SCHEDULE);});
    serve("DELETE", `${BASE}/integration-grants/${GRANT_ID}`, async () => {await writes.promise; return response(null, 204);});
    serve("GET", `${BASE}/schedules`, async () => {await reads.promise; return response({schedules: [failed]});});
    serve("GET", `${BASE}/integration-grants`, async () => {await reads.promise; return response({items: [GITHUB, SENTRY]});});
    function Content({settled}: {settled: number}) {useReloadListsAfter(settled); return <FleetListSections />;}
    const tree = (settled: number) => <TooltipProvider><FleetListsProvider workspace={WORKSPACE} fleet={FLEET_ID} initial={detail}>
      <Content settled={settled} />
    </FleetListsProvider></TooltipProvider>;
    const view = render(tree(0));
    fireEvent.click(screen.getByRole("button", {name: "Retry registration"}));
    fireEvent.click(screen.getByRole("button", {name: "Revoke access: github"}));
    fireEvent.click(within(await screen.findByRole("alertdialog")).getByRole("button", {name: "Revoke access"}));
    view.rerender(tree(1));
    view.rerender(tree(2));
    expect(calls.filter(call => call.path === `${BASE}/integration-grants`)).toHaveLength(2);
    await act(async () => writes.resolve());
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    expect(screen.queryByText("Registration failed")).toBeNull();
    expect(screen.queryByRole("button", {name: "Revoke access: github"})).toBeNull();
    await act(async () => reads.resolve());
    expect(await screen.findByRole("button", {name: "Revoke access: sentry"})).toBeTruthy();
    expect(screen.queryByRole("button", {name: "Revoke access: github"})).toBeNull();
    expect(screen.queryByText("Registration failed")).toBeNull();
    expect(screen.getByText(SCHEDULE.description)).toBeTruthy();
    expect(screen.getByText(PIN.content)).toBeTruthy();
  });

  it("leaves a list that is still loading alone when a removal lands, and drops late reads after unmount", async () => {
    const late = Promise.withResolvers<Response>();
    lists();
    serve("GET", `${BASE}/triggers`, () => late.promise);
    serve("DELETE", `${BASE}/triggers/${TRIGGER_ID}`, () => response(null, 204));
    const seen: {kind: string}[] = [];
    let remove: ReturnType<typeof useFleetLists> = null;
    function Probe() {
      const owner = useFleetLists();
      remove = owner;
      if (owner) seen.push(owner.triggers);
      return null;
    }
    const view = render(<FleetListsProvider workspace={WORKSPACE} fleet={FLEET_ID}><Probe /></FleetListsProvider>);
    await waitFor(() => expect(calls).toHaveLength(4));
    expect(await act(async () => remove?.remove({kind: "trigger", trigger: {...WEBHOOK, events: null, repositories: null, channel: null}}))).toBeNull();
    expect(seen.at(-1)).toEqual({kind: "loading"});
    view.unmount();
    await act(async () => late.resolve(response({items: [WEBHOOK]})));
    expect(seen.at(-1)).toEqual({kind: "loading"});
  });

  it("reads the lists again only when another turn settles", async () => {
    lists({schedules: [], triggers: [], grants: []});
    function Turns({settled}: {settled: number}) {useReloadListsAfter(settled); return null;}
    const tree = (settled: number) => <TooltipProvider><FleetListsProvider workspace={WORKSPACE} fleet={FLEET_ID} initial={detail}>
      <Turns settled={settled} /><FleetListSections />
    </FleetListsProvider></TooltipProvider>;
    const view = render(tree(2));
    expect(screen.getByText("Only a message wakes this fleet.")).toBeTruthy();
    await screen.findByText(PIN.content);
    view.rerender(tree(2));
    view.rerender(tree(1));
    expect(calls).toHaveLength(1);
    serve("GET", `${BASE}/schedules`, () => response({schedules: [SCHEDULE]}));
    serve("GET", `${BASE}/triggers`, () => response({items: [WEBHOOK]}));
    serve("GET", `${BASE}/integration-grants`, () => response({items: [SENTRY]}));
    view.rerender(tree(3));
    expect(await screen.findByText(SCHEDULE.description)).toBeTruthy();
    expect(await screen.findByText("Webhook · sentry")).toBeTruthy();
    expect(await screen.findByRole("button", {name: "Revoke access: sentry"})).toBeTruthy();
    render(<Turns settled={1} />).rerender(<Turns settled={5} />);
    expect(calls).toHaveLength(5);
  });

  it("discards the previous viewer's fleet detail and late reads on an identity change", async () => {
    lists();
    const oldMemory = Promise.withResolvers<Response>();
    serve("GET", `${BASE}/memories?limit=100`, () => oldMemory.promise);
    const tree = (viewer: string) => <StrictMode><TooltipProvider><FleetWorkspace account="account" workspace={WORKSPACE} fleet={detail} models={[MODEL]} viewer={viewer}>
      <p>{viewer}'s conversation</p><FleetListSections />
    </FleetWorkspace></TooltipProvider></StrictMode>;
    const view = render(tree("person"));
    const initialReads = calls.length;
    expect(screen.getByRole("button", {name: "Revoke access: github"})).toBeTruthy();
    identity.userId = "another person";
    view.rerender(tree("person"));
    view.rerender(tree("person"));
    expect(router.refresh).toHaveBeenCalledTimes(1);
    expect(calls).toHaveLength(initialReads);
    expect(screen.getByLabelText("Loading page")).toBeTruthy();
    expect(screen.queryByText("person's conversation")).toBeNull();
    expect(screen.queryByText(SCHEDULE.description)).toBeNull();
    expect(screen.queryByText("Webhook · sentry")).toBeNull();
    expect(screen.queryByRole("button", {name: "Revoke access: github"})).toBeNull();
    lists({schedules: [], triggers: [], grants: [SENTRY], pinned: []});
    view.rerender(tree("another person"));
    expect(screen.getByText("another person's conversation")).toBeTruthy();
    expect(await screen.findByRole("button", {name: "Revoke access: sentry"})).toBeTruthy();
    await act(async () => oldMemory.resolve(response({items: [PIN], total: 1, next_cursor: null})));
    expect(screen.queryByText(PIN.content)).toBeNull();
    expect(calls.map(call => call.path)).toEqual(Array.from({length: initialReads * 2}, () => `${BASE}/memories?limit=100`));
  });

  it("checks each unseeded list on mount and shows the endpoint's refusal again after refresh", async () => {
    lists();
    const denied = () => response({user_message: "Missing scope."}, 403);
    serve("GET", `${BASE}/schedules`, denied);
    serve("GET", `${BASE}/integration-grants`, denied);
    const fleet = {...detail, schedules: undefined, grants: undefined};
    render(<TooltipProvider><FleetWorkspace account="account" workspace={WORKSPACE} fleet={fleet} models={[MODEL]} viewer="person">
      <FleetListSections />
    </FleetWorkspace></TooltipProvider>);
    expect(screen.queryByText(SCHEDULE.description)).toBeNull();
    expect(screen.queryByRole("button", {name: "Revoke access: github"})).toBeNull();
    expect(await within(section("Can access")).findByText(/doesn’t have access here/)).toBeTruthy();
    expect(await within(section("Wakes on")).findByText(/doesn’t have access here/)).toBeTruthy();
    expect(calls.map(call => call.path).sort()).toEqual([`${BASE}/integration-grants`, `${BASE}/memories?limit=100`, `${BASE}/schedules`]);
    fireEvent.click(within(section("Can access")).getByRole("button", {name: "Try again"}));
    await waitFor(() => expect(calls).toHaveLength(7));
    expect(within(section("Can access")).getByText(/doesn’t have access here/)).toBeTruthy();
    expect(screen.queryByRole("button", {name: "Revoke access: github"})).toBeNull();
  });
});

describe("the fleet's own fields", () => {
  it("a refused network toggle stays off", async () => {
    lists();
    serve("PATCH", BASE, () => response({user_message: "Only a signed-in person can open a fleet's network."}, 403));
    page();
    const allowAll = screen.getByRole("switch", {name: "Allow all public internet access"});
    fireEvent.click(allowAll);
    expect(await screen.findByRole("alert")).toBeTruthy();
    const patches = calls.filter(call => call.method === "PATCH");
    expect(patches).toEqual([expect.objectContaining({path: BASE, body: {http_egress: "public"}, headers: expect.objectContaining({"If-Match": FLEET_DETAIL.etag})})]);
    expect(allowAll.getAttribute("aria-checked")).toBe("false");
    expect(allowAll).toHaveProperty("disabled", false);
    expect(screen.getByRole("alert").textContent).toContain("doesn’t have access here");
  });

  it("closes an open network with one patch", async () => {
    lists();
    serve("PATCH", BASE, () => response({...FLEET_DETAIL, http_egress: "restricted", etag: '"v2"'}));
    page(undefined, {...detail, http_egress: "public"});
    const allowAll = screen.getByRole("switch", {name: "Allow all public internet access"});
    expect(allowAll.getAttribute("aria-checked")).toBe("true");
    fireEvent.click(allowAll);
    await waitFor(() => expect(screen.getByRole("switch", {name: "Allow all public internet access"}).getAttribute("aria-checked")).toBe("false"));
    expect(calls.filter(call => call.method === "PATCH").map(call => call.body)).toEqual([{http_egress: "restricted"}]);
  });

  it("moves a switch once the daemon confirms it, and sends the next change with the version it answered", async () => {
    lists();
    let version = 1;
    serve("PATCH", BASE, init => {
      const change = z.record(z.string(), z.unknown()).parse(typeof init.body === "string" ? JSON.parse(init.body) : {});
      version += 1;
      return response({...FLEET_DETAIL, package_domains: true, ...change, etag: `"v${version}"`});
    });
    page();
    fireEvent.click(screen.getByRole("switch", {name: "Packages"}));
    await waitFor(() => expect(screen.getByRole("switch", {name: "Packages"}).getAttribute("aria-checked")).toBe("true"));
    fireEvent.click(screen.getByRole("switch", {name: "Allow all public internet access"}));
    await waitFor(() => expect(screen.getByRole("switch", {name: "Packages"})).toHaveProperty("disabled", true));
    const patches = calls.filter(call => call.method === "PATCH");
    expect(patches.map(call => [call.body, (call.headers as Record<string, string>)["If-Match"]])).toEqual([
      [{package_domains: true}, FLEET_DETAIL.etag], [{http_egress: "public"}, '"v2"']]);
  });

  it("names a stale version instead of retrying it, and sends one change at a time", async () => {
    lists();
    const held = Promise.withResolvers<Response>();
    serve("PATCH", BASE, () => held.promise);
    page();
    fireEvent.click(screen.getByRole("switch", {name: "Packages"}));
    await waitFor(() => expect(screen.getByRole("switch", {name: "Packages"})).toHaveProperty("disabled", true));
    fireEvent.click(screen.getByRole("combobox", {name: "Fleet model"}));
    fireEvent.click(await screen.findByRole("option", {name: "Other model"}));
    await act(async () => held.resolve(response({user_message: "Fleet changed."}, 412)));
    expect((await screen.findByRole("alert")).textContent).toBe("Fleet changed.");
    expect(calls.filter(call => call.method === "PATCH")).toHaveLength(1);
  });

  it("changes the model with one patch and shows the confirmed label, and ignores the model it already has", async () => {
    lists();
    serve("PATCH", BASE, () => response({...FLEET_DETAIL, model_ref: "model:other", etag: '"v2"'}));
    page();
    const picker = screen.getByRole("combobox", {name: "Fleet model"});
    expect(picker.textContent).toContain(MODEL.label);
    fireEvent.click(picker);
    fireEvent.click(await screen.findByRole("option", {name: MODEL.label}));
    expect(calls.some(call => call.method === "PATCH")).toBe(false);
    fireEvent.click(screen.getByRole("combobox", {name: "Fleet model"}));
    fireEvent.click(await screen.findByRole("option", {name: "Other model"}));
    await waitFor(() => expect(screen.getByRole("combobox", {name: "Fleet model"}).textContent).toContain("Other model"));
    expect(calls.filter(call => call.method === "PATCH").map(call => call.body)).toEqual([{model_ref: "model:other"}]);
  });

  it("keeps the model and says why when a change is refused", async () => {
    lists();
    const held = Promise.withResolvers<Response>();
    serve("PATCH", BASE, () => held.promise);
    page();
    fireEvent.click(screen.getByRole("combobox", {name: "Fleet model"}));
    fireEvent.click(await screen.findByRole("option", {name: "Other model"}));
    expect(await screen.findByText("Saving model…")).toBeTruthy();
    await act(async () => held.resolve(response({user_message: "That model is not available to this workspace.", error_code: "UZ-SETUP-008"}, 409)));
    expect(await screen.findByText("That model is not available to this workspace.")).toBeTruthy();
    expect(screen.getByRole("combobox", {name: "Fleet model"}).textContent).toContain(MODEL.label);
    expect(screen.queryByRole("alert", {name: /network/i})).toBeNull();
  });

  it("names the stream's owner by the server's viewer until the client knows the person", async () => {
    identity.isLoaded = false;
    lists();
    page();
    expect(await screen.findByText("Always link the evidence.")).toBeTruthy();
  });

  it("renders nothing for a signed-out person", () => {
    identity.userId = null;
    const view = page(<p>fleet</p>);
    expect(view.container.textContent).toBe("");
    expect(calls).toEqual([]);
  });

  it("offers no picker choices or switches outside a fleet", () => {
    render(<><FleetModelSelect label="Saved model" /><FleetNetworkSection /><FleetListSections /></>);
    expect(screen.getByText("Saved model")).toBeTruthy();
    expect(screen.queryByRole("switch")).toBeNull();
    expect(screen.queryByRole("region")).toBeNull();
  });
});
