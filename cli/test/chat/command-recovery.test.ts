import { expect, test } from "bun:test";
import { runCli } from "../../src/cli.ts";
import { bufferStream, cliEnv, withAuthedStateDir } from "../helpers-cli-state.ts";
import type { Schedule } from "../../src/commands/chat/protocol.ts";
import { CANDIDATE_ID, DRAFT, DRAFT_ID, EMPTY_PAGE, FAILED_SCHEDULE, FLEET, FLEET_ID, LIBRARY_ID, PUBLICATION,
  REVIEW, SCHEDULE_RETRY, SCHEDULE_UNCONFIRMED, SYNC_FAILED_KIND, SYNC_UNCONFIRMED_KIND, SYNCED_SCHEDULE,
  SYNCING_SCHEDULE, UNKNOWN_SYNC_SCHEDULE, WORKSPACE, withApi, problem } from "./fixture.ts";

const RUN_TEXT = "private initial runtime request";
const KEY = "0199a801-1000-7000-8000-000000000009";
const JSON_INDENT = 2;
const SAVE_ARGS = [DRAFT_ID, "--no-input", "--save", "--review-digest", REVIEW.review_digest];
const APPLY_ARGS = [DRAFT_ID, "--no-input", "--apply", "--review-digest", REVIEW.review_digest];
const LINKED_DRAFT = {...DRAFT, linked_fleet_id: FLEET_ID, review: {...REVIEW,
  effective: {...REVIEW.effective, target_setup_revision: FLEET.setup_revision}}};
type Call = { path: string; key: string | null; tag: string | null; body: unknown };

const JSON_FORMAT = ["--json"];

async function invoke(origin: string, argv: string[], format = JSON_FORMAT) {
  return withAuthedStateDir({ workspaceId: WORKSPACE, apiUrl: origin }, async () => {
    const stdout = bufferStream();
    const stderr = bufferStream();
    const code = await runCli(["--api", origin, ...format, "chat", ...argv], {
      stdout: stdout.stream, stderr: stderr.stream, env: cliEnv({ NO_COLOR: "1" }),
    });
    return { code, out: stdout.read(), err: stderr.read() };
  });
}

async function record(request: Request): Promise<Call> {
  return {path: new URL(request.url).pathname, key: request.headers.get("Idempotency-Key"),
    tag: request.headers.get("If-Match"), body: await request.json()};
}

test("a separate invocation replays unknown Save before a fresh review and keeps initial text private in diagnostics", async () => {
  const calls: Call[] = [];
  const reads: string[] = [];
  await withApi(async (request) => {
    const path = new URL(request.url).pathname;
    if (request.method === "POST") {
      calls.push(await record(request));
      return calls.length === 1 ? problem(503) : Response.json(FLEET);
    }
    reads.push(path);
    return Response.json(path.endsWith("/messages") ? EMPTY_PAGE : DRAFT);
  }, async (_, origin) => {
    const failed = await invoke(origin, [DRAFT_ID, "--no-input", "--save", "--review-digest", REVIEW.review_digest, "--run-now", RUN_TEXT]);
    expect(failed.code).not.toBe(0);
    expect(failed.err).toContain("setup_operation_recovery");
    expect(failed.err).not.toContain(RUN_TEXT);
    const key = calls[0]?.key;
    expect(key).toBeTruthy();
    if (!key) throw new Error("missing original Save key");
    expect(failed.err).toContain(key);
    const observed = reads.length;
    const recovered = await invoke(origin, ["--no-input", "--save", "--operation-id", key,
      "--candidate-id", CANDIDATE_ID, "--name", REVIEW.effective.instance_name,
      "--review-digest", REVIEW.review_digest, "--run-now", RUN_TEXT]);
    expect(recovered.code).toBe(0);
    expect(calls[0]).toEqual(calls[1]);
    expect(reads).toHaveLength(observed);
    expect(recovered.out).toContain(FLEET_ID);
  });
});

test("unknown Apply replays its original setup tag after the live fleet has advanced", async () => {
  const calls: Call[] = [];
  await withApi(async (request) => {
    const path = new URL(request.url).pathname;
    if (request.method === "POST") {
      calls.push(await record(request));
      return calls.length === 1 ? problem(503) : Response.json({fleet_id: FLEET_ID,
        setup_revision: "2", setup_etag: '"setup-2"', etag: FLEET.etag, schedules: []});
    }
    if (path.endsWith("/messages")) return Response.json(EMPTY_PAGE);
    if (path.endsWith(FLEET_ID)) return Response.json({...FLEET, setup_etag: calls.length ? '"new-tag"' : FLEET.setup_etag});
    return Response.json({...DRAFT, linked_fleet_id: FLEET_ID, review: {...REVIEW,
      effective: {...REVIEW.effective, target_setup_revision: "1"}}});
  }, async (_, origin) => {
    const failed = await invoke(origin, [DRAFT_ID, "--no-input", "--apply", "--review-digest", REVIEW.review_digest]);
    expect(failed.code).not.toBe(0);
    const key = calls[0]?.key;
    if (!key) throw new Error("missing original Apply key");
    expect(failed.err).toContain("setup_etag");
    const result = await invoke(origin, ["--no-input", "--apply", "--operation-id", key,
      "--candidate-id", CANDIDATE_ID, "--fleet", FLEET_ID,
      "--review-digest", REVIEW.review_digest, "--setup-etag", FLEET.setup_etag]);
    expect(result.code).toBe(0);
    expect(calls[0]).toEqual(calls[1]);
    expect(calls[1]?.tag).toBe(FLEET.setup_etag);
  });
});

test("owned publication recovery preserves candidate, key and inspected Library tag", async () => {
  const calls: Call[] = [];
  await withApi(async (request) => {
    if (request.method === "POST") {
      calls.push(await record(request));
      return calls.length === 1 ? problem(503) : Response.json(PUBLICATION);
    }
    return Response.json(new URL(request.url).pathname.endsWith("/messages") ? EMPTY_PAGE : DRAFT);
  }, async (_, origin) => {
    const args = ["--no-input", "--publish", "Argo", "--publish-to", LIBRARY_ID, "--library-etag", PUBLICATION.etag];
    expect((await invoke(origin, [DRAFT_ID, ...args, "--review-digest", REVIEW.review_digest])).code).not.toBe(0);
    const key = calls[0]?.key;
    if (!key) throw new Error("missing original publication key");
    expect((await invoke(origin, [...args, "--operation-id", key, "--candidate-id", CANDIDATE_ID])).code).toBe(0);
    expect(calls[0]).toEqual(calls[1]);
    expect(calls[1]?.tag).toBe(PUBLICATION.etag);
  });
});

test("a partial Save emits a usable operation identity and exits unsuccessful", async () => {
  await withApi((request) => Response.json(request.method === "POST" ? {...FLEET, status: "installing"}
    : new URL(request.url).pathname.endsWith("/messages") ? EMPTY_PAGE : DRAFT), async (_, origin) => {
    const result = await invoke(origin, [DRAFT_ID, "--no-input", "--save", "--review-digest", REVIEW.review_digest]);
    expect(result.code).toBe(1);
    expect(result.err).toContain("setup_operation_recovery");
    expect(result.err).toContain(CANDIDATE_ID);
    expect(result.err).toContain(REVIEW.review_digest);
  });
});

function savedWith(schedules: unknown[]) {
  return (request: Request) => Response.json(request.method === "POST" ? {...FLEET, schedules}
    : new URL(request.url).pathname.endsWith("/messages") ? EMPTY_PAGE : DRAFT);
}

function appliedWith(schedules: unknown[]) {
  return (request: Request) => {
    const path = new URL(request.url).pathname;
    if (request.method === "POST") return Response.json({fleet_id: FLEET_ID, setup_revision: "2",
      setup_etag: '"setup-2"', etag: FLEET.etag, schedules});
    return Response.json(path.endsWith("/messages") ? EMPTY_PAGE : path.endsWith(FLEET_ID) ? FLEET : LINKED_DRAFT);
  };
}

// Diagnostics are indented JSON documents on stderr, one per finding.
function expectOwed(result: {code: number; err: string}, kind = SYNC_FAILED_KIND, schedule: Schedule = FAILED_SCHEDULE) {
  expect(result.code).toBe(1);
  const owed = JSON.stringify({kind, fleet_id: FLEET_ID, schedule_id: schedule.schedule_id,
    last_error: schedule.last_error ?? null, retry: SCHEDULE_RETRY}, null, JSON_INDENT);
  expect(result.err).toContain(owed);
}

test("an active Save whose schedule failed to register is partial, names the retry, and exits unsuccessful", async () => {
  await withApi(savedWith([SYNCED_SCHEDULE, FAILED_SCHEDULE]), async (_, origin) => {
    const result = await invoke(origin, SAVE_ARGS);
    expectOwed(result);
    expect(result.out).toContain(FAILED_SCHEDULE.sync);
  });
});

test("an active Save whose schedules all registered is complete and exits zero", async () => {
  await withApi(savedWith([SYNCED_SCHEDULE]), async (_, origin) => {
    const result = await invoke(origin, SAVE_ARGS);
    expect(result.code).toBe(0);
    expect(result.err).not.toContain(SYNC_FAILED_KIND);
  });
});

test("an Apply whose schedule failed to register is partial and exits unsuccessful; a synced one exits zero", async () => {
  await withApi(appliedWith([FAILED_SCHEDULE]), async (_, origin) => {
    expectOwed(await invoke(origin, APPLY_ARGS));
  });
  await withApi(appliedWith([SYNCED_SCHEDULE]), async (_, origin) => {
    const result = await invoke(origin, APPLY_ARGS);
    expect(result.code).toBe(0);
    expect(result.err).not.toContain(SYNC_FAILED_KIND);
  });
});

test("a Save whose schedule registration was never confirmed is partial, names the retry, and exits unsuccessful", async () => {
  await withApi(savedWith([SYNCED_SCHEDULE, SYNCING_SCHEDULE]), async (_, origin) => {
    const result = await invoke(origin, SAVE_ARGS, []);
    expect(result.code).toBe(1);
    expect(result.err).toContain(SCHEDULE_UNCONFIRMED);
  });
});

test("an Apply whose schedule is still syncing, or in a status this client does not know, exits unsuccessful", async () => {
  await withApi(appliedWith([SYNCING_SCHEDULE]), async (_, origin) => {
    expectOwed(await invoke(origin, APPLY_ARGS), SYNC_UNCONFIRMED_KIND, SYNCING_SCHEDULE);
  });
  await withApi(appliedWith([UNKNOWN_SYNC_SCHEDULE]), async (_, origin) => {
    expectOwed(await invoke(origin, APPLY_ARGS), SYNC_UNCONFIRMED_KIND, UNKNOWN_SYNC_SCHEDULE);
  });
});

test("operation recovery reports a failed schedule from a replayed Save or Apply", async () => {
  const recover = ["--no-input", "--operation-id", KEY, "--candidate-id", CANDIDATE_ID, "--review-digest", REVIEW.review_digest];
  await withApi(savedWith([FAILED_SCHEDULE]), async (_, origin) => {
    expectOwed(await invoke(origin, [...recover, "--save", "--name", REVIEW.effective.instance_name]));
  });
  await withApi(appliedWith([FAILED_SCHEDULE]), async (_, origin) => {
    expectOwed(await invoke(origin, [...recover, "--apply", "--fleet", FLEET_ID, "--setup-etag", FLEET.setup_etag]));
  });
});

test("publication requires the digest the caller inspected before any mutation", async () => {
  const posts: string[] = [];
  await withApi((request) => {
    if (request.method === "POST") posts.push(request.url);
    return Response.json(new URL(request.url).pathname.endsWith("/messages") ? EMPTY_PAGE : DRAFT);
  }, async (_, origin) => {
    expect((await invoke(origin, [DRAFT_ID, "--no-input", "--publish", "Argo"])).code).not.toBe(0);
    expect(posts).toEqual([]);
  });
});

for (const args of [["--save"], ["--save", "--name", "argo"], ["--apply", "--fleet", FLEET_ID],
  ["--publish", "Argo", "--publish-to", LIBRARY_ID]]) {
  test(`incomplete recovery metadata refuses before network work: ${args.join(" ")}`, async () => {
    const seen: string[] = [];
    await withApi((request) => { seen.push(request.url); return problem(500); }, async (_, origin) => {
      const result = await invoke(origin, ["--no-input", "--operation-id", KEY, "--candidate-id", CANDIDATE_ID, ...args]);
      expect(result.code).not.toBe(0);
      expect(seen).toEqual([]);
    });
  });
}
