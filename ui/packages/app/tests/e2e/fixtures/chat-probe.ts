import { execFile } from "node:child_process";
import http from "node:http";
import path from "node:path";
import { promisify } from "node:util";
import { test as base } from "@playwright/test";
import { applicationCss } from "./fleet-output-browser-assets";
import { summary, detail } from "@/tests/helpers/task-reads";
import { FLEET_DETAIL, FLEET_ID, WORKSPACE } from "@/tests/helpers/fleet-fixtures";
import { SCHEDULE_TURN } from "@/tests/helpers/schedule-activity";
import { visualDurableTaskDetail, visualDurableTaskPage } from "./refresh-visual-data";
import { visualWakeup } from "./refresh-wakeup-data";
import { RESEARCH, researchTurn } from "./web-research-data";
import { NEW_FLEET, newFleetTurn } from "./new-fleet-data";

const LIVE = `/live/v1/workspaces/${WORKSPACE}`;
const FLEET_LIST = `${LIVE}/fleets`;
// `GET F/<list>`: the lists beside a fleet's chat. Empty unless a scenario names rows.
const FLEET_CHILD = new RegExp(`^${LIVE}/fleets/([^/]+)/(schedules|triggers|integration-grants|memories|messages|turns/stream)$`);
const HEARTBEAT = 'event: heartbeat\ndata: {"kind":"heartbeat"}\n\n';

export const test = base.extend<object, {chatProbe: string}>({
  chatProbe: [async ({browserName}, provideProbe) => {
    const appRoot = path.resolve(import.meta.dirname, "../../..");
    const built = await promisify(execFile)("bun", ["tests/e2e/fixtures/chat-browser-build.ts"],
      {cwd: appRoot, maxBuffer: 4_000_000, timeout: 20_000});
    const css = await applicationCss(appRoot);
    // The New fleet scenario's fleets, by id, with the first message that made each. A page load starts with none.
    const created = new Map<string, {name: string; message: string}>();
    const server = http.createServer((request, response) => {
      const url = new URL(request.url ?? "/", "http://localhost");
      const source = new URL(request.headers.referer ?? "http://localhost");
      const json = (body: unknown, status = 200) => {response.writeHead(status, {"content-type": "application/json"}); response.end(JSON.stringify(body));};
      const child = FLEET_CHILD.exec(url.pathname);
      if (url.pathname === FLEET_LIST && request.method === "POST") {
        // A first message makes the scenario's fleet; a repeat of one operation answers the same fleet.
        let body = "";
        request.on("data", chunk => {body += String(chunk);});
        request.on("end", () => {
          const {message} = JSON.parse(body) as {message: string};
          const replayed = created.has(NEW_FLEET.id);
          created.set(NEW_FLEET.id, {name: NEW_FLEET.name, message});
          json({fleet: {...FLEET_DETAIL, id: NEW_FLEET.id, name: NEW_FLEET.name}, accepted: {turn_id: NEW_FLEET.turnId, disposition: "started"}, replayed}, replayed ? 200 : 201);
        });
      } else if (url.pathname === FLEET_LIST && request.method === "GET" && source.searchParams.has("research")) {
        json({items: [{...FLEET_DETAIL, id: FLEET_ID, name: RESEARCH.name, status: "active"}], total: null, next_cursor: null});
      } else if (url.pathname === FLEET_LIST && request.method === "GET" && source.searchParams.has("newFleet")) {
        json({items: [...created].map(([id, fleet]) => ({...FLEET_DETAIL, id, name: fleet.name})), total: null, next_cursor: null});
      } else if (url.pathname === FLEET_LIST && request.method === "GET" && source.searchParams.has("visual")) {
        const phase = source.searchParams.get("state");
        const name = phase === "wake-up" ? "Merge watch" : phase === "activity" ? "Launch review" : phase === "error" ? "Deploy monitor" : "Release watch";
        json({items: [{...FLEET_DETAIL, name, status: "active"}], total: null, next_cursor: null});
      } else if (url.pathname === FLEET_LIST && request.method === "GET" && source.searchParams.has("alignment")) {
        const extra = source.searchParams.has("scale") ? Array.from({length: 11}, (_, index) => ({
          ...FLEET_DETAIL, id: `0199a801-1000-7000-8000-${String(index + 10).padStart(12, "0")}`, name: `review-fleet-${index + 2}`,
        })) : [];
        json({items: [{...FLEET_DETAIL, name: "ci-repairer"}, ...extra], total: null, next_cursor: null});
      } else if (child?.[2] === "turns/stream") {
        response.writeHead(200, {"content-type": "text/event-stream", "cache-control": "no-cache"});
        response.write(HEARTBEAT);
      } else if (child?.[2] === "messages" && source.searchParams.has("research")) {
        json({items: [researchTurn(SCHEDULE_TURN, source.searchParams.has("plain"))], total: null, next_cursor: null});
      } else if (child?.[2] === "messages") {
        // The new fleet's thread: its first message, answered.
        const made = created.get(child[1] ?? "");
        json({items: made ? [newFleetTurn(SCHEDULE_TURN, made.message)] : [], total: null, next_cursor: null});
      } else if (child?.[2] === "schedules") {
        json({schedules: source.searchParams.get("state") === "wake-up" ? [visualWakeup(Date.now())] : []});
      } else if (child?.[2] === "memories") {
        json({items: [], total: 0, next_cursor: null});
      } else if (child) {
        json({items: []});
      } else if (request.url === "/probe.js") {
        response.writeHead(200, {"content-type": "text/javascript"}); response.end(built.stdout);
      } else if (request.url?.includes("/durable_tasks")) {
        if (source.searchParams.has("visual")) {
          const selected = source.searchParams.get("state");
          const scenario = selected === "activity" || selected === "wake-up" || selected === "empty" || selected === "completed" || selected === "error" ? selected : "running";
          const started = Date.now();
          json(request.url.includes("?limit=") ? visualDurableTaskPage(scenario, started) : visualDurableTaskDetail(scenario, started));
          return;
        }
        if (source.searchParams.has("newFleet") || source.searchParams.has("research")) {json({items: [], next_cursor: null}); return;}
        const task = {...summary(undefined, "running"), title: "Validate the Linkwarden repair"};
        if (!url.searchParams.has("limit")) response.setHeader("etag", `"${"a".repeat(64)}"`);
        json(request.url.includes("?limit=") ? {items: [task], next_cursor: null} : detail(task));
      } else if (request.url?.startsWith("/turn-details?")) {
        const query = url.searchParams;
        const owned = query.get("workspace") === SCHEDULE_TURN.workspace_id && query.get("fleet") === SCHEDULE_TURN.fleet_id;
        json({...SCHEDULE_TURN, turn_id: query.get("turn"), response_text: "Native recorded support review."}, owned ? 200 : 404);
      } else {
        if (request.headers["sec-fetch-dest"] === "document") created.clear();
        response.writeHead(200, {"content-type": "text/html"});
        response.end(`<html data-theme="dark"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><style>${css}</style></head><body><div id="root"></div><script type="module" src="/probe.js"></script></body></html>`);
      }
    });
    await new Promise<void>(resolve => server.listen(0, "127.0.0.1", resolve));
    const address = server.address();
    if (!address || typeof address === "string") throw new Error(`${browserName} chat fixture did not bind a TCP port`);
    try {await provideProbe(`http://127.0.0.1:${address.port}`);}
    finally {server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve()));}
  }, {scope: "worker"}],
});
