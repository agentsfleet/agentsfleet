import { expect, test } from "bun:test";
import { runCli } from "../../src/cli.ts";
import { LIBRARY_PAGE_LIMIT, LIBRARY_VISIBILITY } from "../../src/commands/fleet-library-read.ts";
import { bufferStream, cliEnv, withAuthedStateDir } from "../helpers-cli-state.ts";
import { LIBRARY_ID, REVISION_ID, WORKSPACE, withApi, problem } from "./fixture.ts";

const CARD = {
  id: LIBRARY_ID, name: "argo", description: "Review deployments", visibility: LIBRARY_VISIBILITY.tenant,
  created_at: "1970-01-01T00:00:00.001Z", requirements: {}, required_credentials_reasons: {}, head_revision_id: REVISION_ID, etag: '"library-1"',
};
const BASE = `/v1/workspaces/${WORKSPACE}/fleet-libraries`;
const CURSOR = "opaque+/cursor";

async function invoke(origin: string, argv: string[]) {
  return withAuthedStateDir({ workspaceId: WORKSPACE, apiUrl: origin }, async () => {
    const stdout = bufferStream();
    const stderr = bufferStream();
    const code = await runCli(["--api", origin, ...argv], {
      stdout: stdout.stream, stderr: stderr.stream, env: cliEnv({ NO_COLOR: "1" }),
    });
    return { code, out: stdout.read(), err: stderr.read() };
  });
}

for (const [verb, suffix] of [[[], ""], [["list"], "/owned"]] as const) {
  test(`Library ${suffix || "gallery"} preserves native rows and an explicit cursor`, async () => {
    const seen: string[] = [];
    await withApi((request) => {
      seen.push(new URL(request.url).pathname + new URL(request.url).search);
      expect(request.headers.get("authorization")).toMatch(/^Bearer /u);
      return Response.json({ items: [CARD], next_cursor: CURSOR });
    }, async (_, origin) => {
      const result = await invoke(origin, ["--json", "library", ...verb, "--limit", "2", "--starting-after", CURSOR]);
      expect(result.code).toBe(0);
      expect(JSON.parse(result.out)).toEqual({ items: [CARD], next_cursor: CURSOR });
      expect(seen).toEqual([`${BASE}${suffix}?limit=2&starting_after=${encodeURIComponent(CURSOR)}`]);
    });
  });
}

test("Library text gives revision and ordinary chat entry without walking hidden pages", async () => {
  let calls = 0;
  await withApi(() => { calls += 1; return Response.json({ items: [CARD], next_cursor: CURSOR }); }, async (_, origin) => {
    const result = await invoke(origin, ["library"]);
    expect(result.code).toBe(0);
    expect(result.out).toContain(REVISION_ID);
    expect(result.out).toContain("agentsfleet chat --library");
    expect(result.out).toContain(`--starting-after ${CURSOR}`);
    expect(calls).toBe(1);
  });
});

test("an empty Library points to explicit chat publication", async () => {
  await withApi(() => Response.json({ items: [], next_cursor: null }), async (_, origin) => {
    const result = await invoke(origin, ["library", "list"]);
    expect(result.code).toBe(0);
    expect(result.out).toContain("/publish");
    expect(result.out).not.toContain("--github");
  });
});

for (const body of [{ items: [] }, { items: [{ id: LIBRARY_ID }], next_cursor: null },
  { items: Array.from({ length: LIBRARY_PAGE_LIMIT + 1 }, () => CARD), next_cursor: null }]) {
  test("malformed native Library pages fail visibly instead of becoming an empty result", async () => {
    await withApi(() => Response.json(body), async (_, origin) => {
      const result = await invoke(origin, ["library", "--json"]);
      expect(result.code).not.toBe(0);
      expect(result.err).toContain("LIBRARY_RESPONSE_INVALID");
      expect(result.out).toBe("");
    });
  });
}

test("owned deletion names the native resource and leaves installed revision pins alone", async () => {
  const seen: string[] = [];
  await withApi((request) => {
    seen.push(`${request.method} ${new URL(request.url).pathname}`);
    return new Response(null, { status: 204 });
  }, async (_, origin) => {
    const result = await invoke(origin, ["--json", "library", "delete", LIBRARY_ID]);
    expect(result.code).toBe(0);
    expect(result.out).toContain('"removed": true');
    expect(seen).toEqual([`DELETE ${BASE}/${LIBRARY_ID}`]);
  });
});

for (const status of [401, 403, 404, 409, 410]) {
  test(`Library deletion propagates ${status} without a success claim`, async () => {
    await withApi(() => problem(status), async (_, origin) => {
      const result = await invoke(origin, ["library", "delete", LIBRARY_ID]);
      expect(result.code).not.toBe(0);
      expect(result.out).not.toContain("no longer");
      expect(result.err).toContain("injected refusal");
    });
  });
}

test("removed bundle commands refuse before any server mutation", async () => {
  const seen: string[] = [];
  await withApi((request) => { seen.push(request.method); return problem(500); }, async (_, origin) => {
    for (const argv of [["install", "--library", "argo"], ["library", "create", "--github", "acme/argo"],
      ["fleet", "update", LIBRARY_ID, "--from", "/tmp/argo"]]) {
      expect((await invoke(origin, argv)).code).not.toBe(0);
    }
    expect(seen).toEqual([]);
  });
});
