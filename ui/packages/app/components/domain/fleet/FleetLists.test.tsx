import type { ReactNode } from "react";
import type { FleetSettingsValue } from "./FleetSettings";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { z } from "zod";
import { TooltipProvider } from "@agentsfleet/design-system";
import { FLEET_DETAIL, FLEET_ID, GRANT_ID, MODEL, SCHEDULE, TRIGGER_ID, WORKSPACE, response } from "@/tests/helpers/fleet-fixtures";
import { FleetWorkspace } from "./FleetWorkspace";
import { FleetListSections } from "./FleetListSections";
import { FleetListsProvider, useFleetLists, useReloadListsAfter } from "./FleetListsProvider";
import { FleetNetworkSection } from "../network/FleetNetworkSection";
import { FleetModelSelect } from "../models/FleetModelSelect";

const identity = vi.hoisted(() => ({userId: "person" as string | null, isLoaded: true}));
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
const READ_ONLY = {...GITHUB, id: "0199a801-1000-7000-8000-0000000000d2", name: "github", scope: {repositories: ["org/docs"], access: "read"}};
const PENDING = {id: "0199a801-1000-7000-8000-0000000000d3", kind: "secret", name: "grafana", status: "pending"};
const PIN = {key: "evidence", content: "Always link the evidence.", category: "core", updated_at: "2026-11-24T10:00:00Z", visibility: "fleet", writer_fleet_id: FLEET_ID};
type Route = (init: RequestInit) => Response | Promise<Response>;
let routes: Map<string, Route>;
let calls: {method: string; path: string; body?: unknown; headers?: HeadersInit}[];

function serve(method: string, path: string, route: Route) {routes.set(`${method} ${path}`, route);}
function lists({schedules = [SCHEDULE], triggers = [WEBHOOK], grants = [GITHUB, PENDING], pinned = [PIN]}: {schedules?: unknown[]; triggers?: unknown[]; grants?: unknown[]; pinned?: unknown[]} = {}) {
  serve("GET", `${BASE}/schedules`, () => response({schedules}));
  serve("GET", `${BASE}/triggers`, () => response({items: triggers}));
  serve("GET", `${BASE}/integration-grants`, () => response({items: grants}));
  serve("GET", `${BASE}/memories?limit=100`, () => response({items: pinned, total: pinned.length, next_cursor: null}));
}
function page(children: ReactNode = <><FleetListSections /><FleetNetworkSection /><FleetModelSelect label="Saved model" /></>, fleet: FleetSettingsValue & {id: string} = FLEET_DETAIL) {
  return render(<TooltipProvider><FleetWorkspace account="account" workspace={WORKSPACE} fleet={fleet} models={[MODEL, {...MODEL, ref: "model:other", label: "Other model"}]} viewer="person">{children}</FleetWorkspace></TooltipProvider>);
}
const section = (name: string) => screen.getByRole("region", {name});

beforeEach(() => {
  routes = new Map(); calls = []; identity.userId = "person"; identity.isLoaded = true;
  vi.stubGlobal("fetch", vi.fn(async (path: string, init: RequestInit = {}) => {
    const method = init.method ?? "GET";
    calls.push({method, path, body: typeof init.body === "string" ? JSON.parse(init.body) : undefined, headers: init.headers});
    const route = routes.get(`${method} ${path}`);
    return route ? route(init) : response({user_message: `unrouted ${method} ${path}`}, 599);
  }));
});
afterEach(() => {cleanup(); vi.unstubAllGlobals();});

describe("the fleet page's lists", () => {
  it("the fleet page asks for no draft or setup", async () => {
    lists();
    page();
    await screen.findByText("Always link the evidence.");
    const reads = calls.map(call => `${call.method} ${call.path}`).sort();
    expect(reads).toEqual([`GET ${BASE}/integration-grants`, `GET ${BASE}/memories?limit=100`, `GET ${BASE}/schedules`, `GET ${BASE}/triggers`]);
    expect(calls.some(call => /\/(?:drafts|setup-context|revisions)\b/.test(call.path))).toBe(false);
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
    expect(screen.getByRole("button", {name: `Remove wake-up: ${SCHEDULE.description}`})).toBeTruthy();
    expect(screen.getByRole("button", {name: "Remove trigger: Webhook · sentry"})).toBeTruthy();
    expect(screen.getByRole("button", {name: "Remove access: sentry"})).toBeTruthy();
    expect(screen.getByRole("button", {name: "Forget pinned memory: evidence"})).toBeTruthy();
  });

  it("says what each empty list means", async () => {
    lists({schedules: [], triggers: [], grants: [PENDING], pinned: []});
    page();
    expect(await screen.findByText("Only a message wakes this fleet.")).toBeTruthy();
    expect(await screen.findByText("Nothing yet. The fleet asks when it needs access.")).toBeTruthy();
    expect(await screen.findByText("Nothing pinned. Ask the fleet to remember standing guidance.")).toBeTruthy();
  });

  it("shows a list that could not be read with a retry, and reads all four again", async () => {
    lists();
    serve("GET", `${BASE}/triggers`, () => response({user_message: "Triggers are unavailable right now."}, 503));
    page();
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
    fireEvent.click(await screen.findByRole("button", {name: "Forget pinned memory: evidence"}));
    const dialog = await screen.findByRole("alertdialog");
    expect(calls.some(call => call.method === "DELETE")).toBe(false);
    fireEvent.click(within(dialog).getByRole("button", {name: "Forget"}));
    await waitFor(() => expect(screen.queryByText("Always link the evidence.")).toBeNull());
    expect(calls.filter(call => call.method === "DELETE").map(call => call.path)).toEqual([`${BASE}/memories/evidence`]);
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(await screen.findByText("Nothing pinned. Ask the fleet to remember standing guidance.")).toBeTruthy();
  });

  it("removes a trigger and an approved grant with one delete each", async () => {
    lists();
    serve("DELETE", `${BASE}/triggers/${TRIGGER_ID}`, () => response(null, 204));
    serve("DELETE", `${BASE}/integration-grants/${GRANT_ID}`, () => response(null, 204));
    page();
    fireEvent.click(await screen.findByRole("button", {name: "Remove trigger: Webhook · sentry"}));
    fireEvent.click(within(await screen.findByRole("alertdialog")).getByRole("button", {name: "Remove trigger"}));
    await waitFor(() => expect(screen.queryByText("Webhook · sentry")).toBeNull());
    fireEvent.click(screen.getByRole("button", {name: "Remove access: github"}));
    fireEvent.click(within(await screen.findByRole("alertdialog")).getByRole("button", {name: "Remove access"}));
    await waitFor(() => expect(screen.queryByRole("button", {name: "Remove access: github"})).toBeNull());
    expect(calls.filter(call => call.method === "DELETE").map(call => call.path)).toEqual([`${BASE}/triggers/${TRIGGER_ID}`, `${BASE}/integration-grants/${GRANT_ID}`]);
  });

  it.each([
    [response({...SCHEDULE, status: "deleting", sync: "syncing"}), "Removal requested"],
    [response(null, 204), null],
  ] as const)("keeps a schedule the scheduler has yet to drop, and drops one it confirmed", async (answer, left) => {
    lists();
    serve("DELETE", `${BASE}/schedules/${SCHEDULE.schedule_id}`, () => answer.clone());
    page();
    fireEvent.click(await screen.findByRole("button", {name: `Remove wake-up: ${SCHEDULE.description}`}));
    fireEvent.click(within(await screen.findByRole("alertdialog")).getByRole("button", {name: "Remove wake-up"}));
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
    fireEvent.click(await screen.findByRole("button", {name: "Remove access: github"}));
    const dialog = await screen.findByRole("alertdialog");
    fireEvent.click(within(dialog).getByRole("button", {name: "Remove access"}));
    expect(await within(dialog).findByText(/doesn’t have access here/)).toBeTruthy();
    expect(screen.getByRole("button", {name: "Remove access: github", hidden: true})).toBeTruthy();
    fireEvent.click(within(dialog).getByRole("button", {name: "Cancel"}));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    fireEvent.click(screen.getByRole("button", {name: "Remove access: github"}));
    expect(within(await screen.findByRole("alertdialog")).queryByText(/doesn’t have access here/)).toBeNull();
  });
});

describe("the lists' owner", () => {
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
    lists();
    function Turns({settled}: {settled: number}) {useReloadListsAfter(settled); return null;}
    const view = render(<FleetListsProvider workspace={WORKSPACE} fleet={FLEET_ID}><Turns settled={2} /></FleetListsProvider>);
    await waitFor(() => expect(calls).toHaveLength(4));
    view.rerender(<FleetListsProvider workspace={WORKSPACE} fleet={FLEET_ID}><Turns settled={2} /></FleetListsProvider>);
    view.rerender(<FleetListsProvider workspace={WORKSPACE} fleet={FLEET_ID}><Turns settled={1} /></FleetListsProvider>);
    expect(calls).toHaveLength(4);
    view.rerender(<FleetListsProvider workspace={WORKSPACE} fleet={FLEET_ID}><Turns settled={3} /></FleetListsProvider>);
    await waitFor(() => expect(calls).toHaveLength(8));
    render(<Turns settled={1} />).rerender(<Turns settled={5} />);
    expect(calls).toHaveLength(8);
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
    page(undefined, {...FLEET_DETAIL, http_egress: "public"});
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
