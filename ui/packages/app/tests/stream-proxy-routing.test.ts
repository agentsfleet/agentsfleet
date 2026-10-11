/**
 * The token-minting proxy routes must never share a path prefix with a
 * rewrite. On Vercel the edge router applies rewrites ahead of same-prefix
 * filesystem route handlers, so a handler under `/backend/*` never runs: the
 * browser's EventSource request goes straight to the API carrying only a
 * cookie, no Bearer, and every stream connect answers 401 UZ-AUTH-002. Local
 * `next start` resolves the handler first, so the defect is invisible until
 * deploy — which is exactly why it needs a test rather than a comment.
 */
import { describe, expect, it, vi } from "vitest";
import nextConfig from "../next.config";
import {
  backfillFleetEventsUrl,
  backfillWorkspaceEventsUrl,
  fleetEventDetailUrl,
  fleetToolCallUrl,
  streamFleetEventsUrl,
  streamWorkspaceEventsUrl,
} from "@/lib/api/events-types";
import { steerMessagesUrl } from "@/lib/api/fleets-types";

// The handlers load their credential and API origin at import; neither is
// reached here, so both stand in.
vi.mock("@clerk/nextjs/server", () => ({ auth: () => Promise.resolve({ getToken: vi.fn() }) }));
vi.mock("@/lib/api/client", () => ({ API_ORIGIN: "https://api.example.test", request: vi.fn() }));

const WORKSPACE_ID = "ws_1";
const FLEET_ID = "z_1";
const EVENT_ID = "1790573387481-566";
const CALL_ID = "7:3";

// Every browser-facing URL whose request MUST be intercepted by a Route
// Handler that injects the api-audience Bearer server-side, with the method
// the browser sends it.
const PROXY_ROUTES: ReadonlyArray<readonly [string, "GET" | "POST"]> = [
  [streamWorkspaceEventsUrl(WORKSPACE_ID), "GET"],
  [streamFleetEventsUrl(WORKSPACE_ID, FLEET_ID), "GET"],
  [backfillWorkspaceEventsUrl(WORKSPACE_ID), "GET"],
  [backfillFleetEventsUrl(WORKSPACE_ID, FLEET_ID), "GET"],
  [fleetEventDetailUrl(WORKSPACE_ID, FLEET_ID, EVENT_ID), "GET"],
  [fleetToolCallUrl(WORKSPACE_ID, FLEET_ID, EVENT_ID, CALL_ID), "GET"],
  [steerMessagesUrl(WORKSPACE_ID, FLEET_ID), "POST"],
];
const PROXY_URLS = PROXY_ROUTES.map(([url]) => url);

// The app-router segment each identifier fills, so a URL names the module
// Next would route it to.
const SEGMENTS: ReadonlyArray<readonly [string, string]> = [
  [WORKSPACE_ID, "[workspaceId]"],
  [FLEET_ID, "[fleetId]"],
  [EVENT_ID, "[eventId]"],
  [encodeURIComponent(CALL_ID), "[callId]"],
];

function routeModule(url: string): string {
  const routePath = (url.split("?")[0] ?? url)
    .split("/")
    .map((part) => SEGMENTS.find(([id]) => id === part)?.[1] ?? part)
    .join("/");
  return `../app${routePath}/route`;
}

async function rewriteSources(): Promise<string[]> {
  const rewrites = await nextConfig.rewrites?.();
  const list = Array.isArray(rewrites) ? rewrites : (rewrites?.beforeFiles ?? []);
  return list.map((rule) => String(rule.source));
}

// "/backend/:path*" → "/backend/" — the literal prefix a request must start
// with for the rewrite to claim it.
function literalPrefix(source: string): string {
  const firstParam = source.indexOf("/:");
  return (firstParam < 0 ? source : source.slice(0, firstParam)) + "/";
}

describe("token-minting proxy routes stay out of rewrite prefixes", () => {
  it("should not serve any proxy URL from a path a rewrite would claim first", async () => {
    const prefixes = (await rewriteSources()).map(literalPrefix);
    expect(prefixes.length).toBeGreaterThan(0);
    for (const url of PROXY_URLS) {
      for (const prefix of prefixes) {
        expect(
          url.startsWith(prefix),
          `${url} sits under the rewrite prefix ${prefix}; on Vercel the rewrite ` +
            "wins and the Bearer-injecting handler never runs (401 UZ-AUTH-002)",
        ).toBe(false);
      }
    }
  });

  it.each(PROXY_ROUTES)("should back %s with a route handler for %s", async (url, method) => {
    // A URL with no handler is the same failure with no local symptom at all.
    const route = (await import(routeModule(url))) as Record<string, unknown>;
    expect(typeof route[method], `the module routing ${url} exports no ${method}`).toBe("function");
  });

  it("should keep every proxy handler on one prefix", () => {
    // Split prefixes are how one route silently drifts back under a rewrite.
    const prefixes = new Set(PROXY_URLS.map((url) => url.split("/")[1]));
    expect(prefixes.size).toBe(1);
  });
});
