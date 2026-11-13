import {afterEach, expect, it, vi} from "vitest";
import {changeSchedule, deleteSchedule, readSchedule} from "./schedule-controls";
import {SCHEDULE, WORKSPACE} from "@/tests/helpers/draft-fixtures";

const current = {...SCHEDULE, source: "trigger" as const, etag: '"revision-2"', override: {message: "Keep this instruction"}};
const signal = () => new AbortController().signal;
afterEach(() => vi.unstubAllGlobals());

// Value: protects=concurrent schedule edits and inherited overlays; fails_when=a pause erases an instruction or writes stale intent;
// why_new=the composer now exposes schedule mutations; seam=HTTP fetch; no_claim=scheduler delivery.
it("pauses an inherited schedule with its current ETag and retains explicit overrides", async () => {
  const fetcher = vi.fn().mockResolvedValueOnce(Response.json(current)).mockResolvedValueOnce(Response.json({...SCHEDULE, status: "paused"}));
  vi.stubGlobal("fetch", fetcher);
  const selected = await readSchedule(WORKSPACE, SCHEDULE, signal());
  await changeSchedule(WORKSPACE, selected, {status: "paused"}, signal());
  const [url, request] = fetcher.mock.calls[1]!;
  expect(url).toContain(`/workspaces/${WORKSPACE}/fleets/${SCHEDULE.fleet_id}/schedules/${SCHEDULE.schedule_id}`);
  expect(request.headers["If-Match"]).toBe(current.etag);
  expect(JSON.parse(request.body)).toEqual({override: {message: "Keep this instruction", status: "paused"}});
  expect(request.credentials).toBe("same-origin");
});

it("edits an independent schedule using the direct fields", async () => {
  const fetcher = vi.fn().mockResolvedValue(Response.json(SCHEDULE));
  vi.stubGlobal("fetch", fetcher);
  await changeSchedule(WORKSPACE, {...current, source: "api"}, {cron: "0 9 * * *", timezone: "UTC"}, signal());
  expect(JSON.parse(fetcher.mock.calls[0]![1].body)).toEqual({cron: "0 9 * * *", timezone: "UTC"});
});

it.each(["fleet_id", "schedule_id"])("refuses a detail read for a different %s", async key => {
  vi.stubGlobal("fetch", vi.fn().mockResolvedValue(Response.json({...current, [key]: "another"})));
  await expect(readSchedule(WORKSPACE, SCHEDULE, signal())).rejects.toThrow(/could not be confirmed/);
});

it("keeps a stale edit refused instead of retrying it against new intent", async () => {
  const fetcher = vi.fn().mockResolvedValue(Response.json({user_message: "Schedule changed"}, {status: 412}));
  vi.stubGlobal("fetch", fetcher);
  await expect(changeSchedule(WORKSPACE, current, {status: "paused"}, signal())).rejects.toMatchObject({status: 412});
  expect(fetcher).toHaveBeenCalledOnce();
});

it("distinguishes pending deregistration from completed deletion", async () => {
  const pending = {...SCHEDULE, status: "deleting", sync: "pending"};
  vi.stubGlobal("fetch", vi.fn().mockResolvedValueOnce(Response.json(pending)).mockResolvedValueOnce(new Response(null, {status: 204})));
  expect(await deleteSchedule(WORKSPACE, SCHEDULE, signal())).toMatchObject(pending);
  expect(await deleteSchedule(WORKSPACE, SCHEDULE, signal())).toBeNull();
});
