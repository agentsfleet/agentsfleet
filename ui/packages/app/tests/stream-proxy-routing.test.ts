/**
 * The token-minting proxy routes must never share a path prefix with a
 * rewrite. On Vercel the edge router applies rewrites ahead of same-prefix
 * filesystem route handlers, so a handler under `/backend/*` never runs: the
 * browser's EventSource request goes straight to the API carrying only a
 * cookie, no Bearer, and every stream connect answers 401 UZ-AUTH-002. Local
 * `next start` resolves the handler first, so the defect is invisible until
 * deploy — which is exactly why it needs a test rather than a comment.
 */
import * as fs from "node:fs";
import * as path from "node:path";
import { describe, expect, it } from "vitest";
import nextConfig from "../next.config";
import {
  backfillFleetEventsUrl,
  backfillWorkspaceEventsUrl,
  streamFleetEventsUrl,
  streamWorkspaceEventsUrl,
} from "@/lib/api/events";

const APP_DIR = path.join(__dirname, "..", "app");
const WORKSPACE_ID = "ws_1";
const FLEET_ID = "z_1";

// Every browser-facing URL whose request MUST be intercepted by a Route
// Handler that injects the api-audience Bearer server-side.
const PROXY_URLS = [
  streamWorkspaceEventsUrl(WORKSPACE_ID),
  streamFleetEventsUrl(WORKSPACE_ID, FLEET_ID),
  backfillWorkspaceEventsUrl(WORKSPACE_ID),
  backfillFleetEventsUrl(WORKSPACE_ID, FLEET_ID),
];

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

function routeHandlerPaths(dir: string, prefix = ""): string[] {
  const found: string[] = [];
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      found.push(...routeHandlerPaths(full, `${prefix}/${entry.name}`));
    } else if (entry.name === "route.ts") {
      found.push(prefix);
    }
  }
  return found;
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

  it("should back every proxy URL with a real route handler on disk", () => {
    // A URL with no handler is the same failure with no local symptom at all.
    const handlers = routeHandlerPaths(APP_DIR).map((p) =>
      p.replace(/\[([^\]]+)\]/g, "PARAM"),
    );
    for (const url of PROXY_URLS) {
      const shape = url
        .split("?")[0]!
        .replace(WORKSPACE_ID, "PARAM")
        .replace(FLEET_ID, "PARAM");
      expect(handlers, `no route handler backs ${url}`).toContain(shape);
    }
  });

  it("should keep every proxy handler on one prefix", () => {
    // Split prefixes are how one route silently drifts back under a rewrite.
    const prefixes = new Set(PROXY_URLS.map((url) => url.split("/")[1]));
    expect(prefixes.size).toBe(1);
  });
});
