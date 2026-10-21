import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const io = vi.hoisted(() => ({inspect: vi.fn(), response: vi.fn(), app: vi.fn(), record: vi.fn(), write: vi.fn()}));
vi.mock("./e2e/acceptance/fixtures/build-binding-io", () => ({inspectBuilds: io.inspect, readBuildResponse: io.response,
  readAppBuild: io.app, readProcessRecord: io.record, writeBuildEvidence: io.write}));
import { assertBuildArtifacts, assertBuildObservation, requireMatchingBuilds } from "./e2e/acceptance/fixtures/build-binding";

const SOURCE = "a".repeat(64);
const OTHER = "b".repeat(64);
const REVISION = "c".repeat(40);
const OTHER_REVISION = "d".repeat(40);
const RUNNER = "0199a801-1000-7000-8000-000000000001";
const PROVIDERS_VERSION = "0.60.0";
const FIRST_BEAT = "2026-10-21T00:00:00.100Z";
const SECOND_BEAT = "2026-10-21T00:00:00.110Z";
const metadata = {schema_latest: 949, schema_hash: SOURCE, definition_schema_version: 1,
  policy_semantics: "fleet-setup-v1", tokenizer_version: "0.12.1", providers_version: PROVIDERS_VERSION, providers_hash: SOURCE};
const manifest = {source_id: SOURCE, revision: REVISION,
  revisions: {daemon: REVISION, runner: REVISION, cli: REVISION, app: REVISION},
  built_at: "2026-10-15T00:00:00.000Z", environment: "isolated",
  daemon_hash: SOURCE, runner_hash: SOURCE, cli_hash: SOURCE, app_hash: SOURCE,
  daemon_path: "/custom/target/debug/agentsfleetd", runner_path: "/custom/target/linux/debug/agentsfleet-runner", metadata};
const inspection = {...manifest, manifest, cli: {build_id: SOURCE}};
const observation = (lastSeen: unknown, id = RUNNER) => ({body: {id, status: "active", last_seen_at: lastSeen, achievable: {build_id: SOURCE}}});

beforeEach(() => {
  vi.resetAllMocks(); vi.useFakeTimers();
  vi.stubEnv("ACCEPTANCE_RUNNER_TOKEN", "synthetic-runner-token");
  vi.stubEnv("ACCEPTANCE_RUNNER_PROCESS_EVIDENCE", "/synthetic/process.json");
  io.inspect.mockResolvedValue(inspection);
  io.record.mockResolvedValue({runner_id: RUNNER, source_id: SOURCE, binary_hash: SOURCE,
    pid: 123, host: "prepared-host", observed_at: new Date().toISOString(), os: "linux", arch: "arm64"});
  io.app.mockResolvedValue(SOURCE);
  io.response.mockResolvedValueOnce({body: {build_id: SOURCE}})
    .mockResolvedValueOnce(observation(FIRST_BEAT)).mockResolvedValueOnce(observation(SECOND_BEAT));
});
afterEach(() => {vi.useRealTimers(); vi.unstubAllEnvs();});

describe("acceptance build binding", () => {
  it("accepts only a complete manifest matching sources and every built output", () => {
    expect(assertBuildArtifacts(inspection)).toEqual(manifest);
  });
  it.each(["source_id", "daemon_hash", "runner_hash", "cli_hash", "app_hash"])("refuses changed %s before process or network reads", async (key) => {
    io.inspect.mockResolvedValue({...inspection, [key]: OTHER});
    await expect(requireMatchingBuilds("http://api.test", "http://app.test")).rejects.toThrow();
    expect(io.record).not.toHaveBeenCalled(); expect(io.response).not.toHaveBeenCalled(); expect(io.write).not.toHaveBeenCalled();
  });
  it("refuses a different checkout revision before process or network reads", async () => {
    io.inspect.mockResolvedValue({...inspection, revision: OTHER_REVISION});
    await expect(requireMatchingBuilds("http://api.test", "http://app.test")).rejects.toThrow("revision differs");
    expect(io.record).not.toHaveBeenCalled(); expect(io.response).not.toHaveBeenCalled(); expect(io.write).not.toHaveBeenCalled();
  });
  it.each(["daemon", "runner", "cli", "app"])("refuses a %s built at another revision", (component) => {
    expect(() => assertBuildArtifacts({...inspection, manifest: {...manifest,
      revisions: {...manifest.revisions, [component]: OTHER_REVISION}}})).toThrow("revision differs");
  });
  it.each([null, undefined, "unknown", OTHER])("refuses an unknown or different serving identity %s", (id) => {
    expect(() => assertBuildObservation("component", id, SOURCE)).toThrow("unknown or differs");
  });
  it("refuses a rebuilt or unbound CLI", () => {
    expect(() => assertBuildArtifacts({...inspection, cli: {build_id: OTHER}})).toThrow("CLI build differs");
    expect(() => assertBuildArtifacts({...inspection, cli: {build_id: null}})).toThrow();
  });
  it.each([-1, 300001])("refuses a future or stale process observation (%s milliseconds)", async (age) => {
    io.record.mockResolvedValue({runner_id: RUNNER, source_id: SOURCE, binary_hash: SOURCE,
      pid: 123, host: "prepared-host", observed_at: new Date(Date.now() - age).toISOString(), os: "linux", arch: "arm64"});
    await expect(requireMatchingBuilds("http://api.test", "http://app.test")).rejects.toThrow("stale");
    expect(io.response).not.toHaveBeenCalled();
  });
  it.each(["schema_hash", "policy_semantics", "tokenizer_version", "providers_hash"])("refuses changed %s before any process or network read", async (key) => {
    io.inspect.mockResolvedValue({...inspection, metadata: {...metadata, [key]: OTHER}});
    await expect(requireMatchingBuilds("http://api.test", "http://app.test")).rejects.toThrow("metadata differs");
    expect(io.record).not.toHaveBeenCalled(); expect(io.response).not.toHaveBeenCalled();
  });
  it("refuses a process record that does not observe a Linux runner", async () => {
    io.record.mockResolvedValue({runner_id: RUNNER, source_id: SOURCE, binary_hash: SOURCE,
      pid: 123, host: "prepared-host", observed_at: new Date().toISOString(), os: "darwin", arch: "arm64"});
    await expect(requireMatchingBuilds("http://api.test", "http://app.test")).rejects.toThrow();
    expect(io.response).not.toHaveBeenCalled(); expect(io.write).not.toHaveBeenCalled();
  });
  it("refuses a mismatched serving daemon before app or runner observations", async () => {
    io.response.mockReset().mockResolvedValue({body: {build_id: OTHER}});
    await expect(requireMatchingBuilds("http://api.test", "http://app.test")).rejects.toThrow("daemon build");
    expect(io.app).not.toHaveBeenCalled(); expect(io.write).not.toHaveBeenCalled();
  });
  it("refuses a mismatched app before runner observation", async () => {
    io.app.mockResolvedValue(OTHER);
    await expect(requireMatchingBuilds("http://api.test", "http://app.test")).rejects.toThrow("app build");
    expect(io.response).toHaveBeenCalledTimes(1); expect(io.write).not.toHaveBeenCalled();
  });
  it("records the matching process and advancing heartbeat after all observations", async () => {
    const run = expect(requireMatchingBuilds("http://api.test", "http://app.test")).resolves.toBeUndefined();
    await vi.runAllTimersAsync(); await run;
    expect(io.response).toHaveBeenCalledTimes(3);
    expect(io.write).toHaveBeenCalledWith(expect.stringContaining(`Heartbeat: ${Date.parse(FIRST_BEAT)} → ${Date.parse(SECOND_BEAT)}`));
    for (const component of ["daemon", "runner", "cli", "app"]) {
      expect(io.write).toHaveBeenCalledWith(expect.stringContaining(`${component} revision: ${REVISION}`));
    }
  });
  it("compares heartbeat instants chronologically across fractional precision", async () => {
    io.response.mockReset().mockResolvedValueOnce({body: {build_id: SOURCE}})
      .mockResolvedValueOnce(observation("2026-10-21T00:00:00.1Z")).mockResolvedValueOnce(observation(SECOND_BEAT));
    const run = expect(requireMatchingBuilds("http://api.test", "http://app.test")).resolves.toBeUndefined();
    await vi.runAllTimersAsync(); await run;
    expect(io.write).toHaveBeenCalledTimes(1);
  });
  it.each([FIRST_BEAT, "2026-10-20T23:59:59.999Z"])("refuses a runner whose heartbeat stayed or moved backwards to %s", async (lastSeen) => {
    io.response.mockReset().mockResolvedValueOnce({body: {build_id: SOURCE}})
      .mockResolvedValueOnce(observation(FIRST_BEAT)).mockResolvedValueOnce(observation(lastSeen));
    const run = expect(requireMatchingBuilds("http://api.test", "http://app.test")).rejects.toThrow("heartbeat did not advance");
    await vi.runAllTimersAsync(); await run;
    expect(io.write).not.toHaveBeenCalled();
  });
  it.each([null, 1, "", "not-a-date", "2026-02-30T00:00:00.000Z", "1970-01-01T00:00:00.000Z", "1969-12-31T23:59:59.999Z"])(
    "refuses invalid heartbeat %j in either observation before recording evidence", async (lastSeen) => {
      for (const first of [true, false]) {
        io.write.mockClear();
        io.response.mockReset().mockResolvedValueOnce({body: {build_id: SOURCE}})
          .mockResolvedValueOnce(observation(first ? lastSeen : FIRST_BEAT))
          .mockResolvedValueOnce(observation(first ? SECOND_BEAT : lastSeen));
        const run = expect(requireMatchingBuilds("http://api.test", "http://app.test")).rejects.toMatchObject({
          name: "ZodError", issues: [expect.objectContaining({path: ["last_seen_at"]})],
        });
        await vi.runAllTimersAsync(); await run;
        expect(io.response).toHaveBeenCalledTimes(first ? 2 : 3);
        expect(io.write).not.toHaveBeenCalled();
      }
    });
});
