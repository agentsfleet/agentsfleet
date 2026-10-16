import {afterEach, beforeEach, expect, spyOn, test} from "bun:test";
import {resolve} from "node:path";
import {observeRunner} from "../../scripts/observe-acceptance-runner.ts";
import {runAcceptanceBuild} from "../../scripts/build-acceptance.ts";
import {fileHash} from "../../scripts/build-source.ts";
import {runnerProcessSchema} from "../../scripts/build-manifest.ts";
import {buildFixture} from "./build-fixture.ts";

const RUNNER = "0199a801-1000-7000-8000-000000000001";
const platform = process.platform;
const envKeys = ["PATH", "ACCEPTANCE_FIXTURE_MODE", "ACCEPTANCE_ENVIRONMENT", "ACCEPTANCE_RUNNER_TARGET", "ACCEPTANCE_BUILD_MANIFEST"];
let previous: Record<string, string | undefined>;
let fixture: Awaited<ReturnType<typeof buildFixture>>;
let fileSpy: ReturnType<typeof spyOn<typeof Bun, "file">> | undefined;
const file = Bun.file.bind(Bun);

beforeEach(async () => {
  previous = Object.fromEntries(envKeys.map(key => [key, Bun.env[key]]));
  fixture = await buildFixture();
  Bun.env.PATH = `${resolve(fixture.root, "bin")}:${Bun.env.PATH}`;
  for (const key of envKeys.slice(1)) delete Bun.env[key];
  Object.defineProperty(process, "platform", {value:"linux"});
});
afterEach(async () => {
  fileSpy?.mockRestore(); fileSpy = undefined;
  Object.defineProperty(process, "platform", {value:platform});
  for (const key of envKeys) { if (previous[key] === undefined) delete Bun.env[key]; else Bun.env[key] = previous[key]; }
  await fixture.cleanup();
});

async function candidate(): Promise<string[]> {
  await runAcceptanceBuild(undefined, fixture.root);
  const destination = resolve(fixture.root, "observation.json");
  return [String(process.pid), RUNNER, destination];
}

test("refuses observation from another operating system", async () => {
  Object.defineProperty(process, "platform", {value:"darwin"});
  await expect(observeRunner([], fixture.root)).rejects.toThrow("actual Linux host");
});

test("requires a process, runner identity and destination", async () => {
  await expect(observeRunner(["0", "invalid", ""], fixture.root)).rejects.toThrow();
});

test("bounds manifest bytes before parsing", async () => {
  const path = resolve(fixture.root, "oversized.json");
  await Bun.write(path, "x".repeat(65_537)); Bun.env.ACCEPTANCE_BUILD_MANIFEST = path;
  await expect(observeRunner([String(process.pid), RUNNER, resolve(fixture.root, "observation.json")], fixture.root)).rejects.toThrow("exceeds its bound");
});

test.each(["matched", "wrong-hash", "not-elf"])("checks live process liveness and executable identity (%s)", async kind => {
  const args = await candidate();
  const executable = resolve(fixture.root, "observed-executable");
  await Bun.write(executable, kind === "matched" ? file(resolve(fixture.root, "dist/agentsfleet-runner"))
    : kind === "not-elf" ? "not a Linux executable" : new Uint8Array([0x7f,0x45,0x4c,0x46,1]));
  // The process is really alive; only its Linux proc-file bytes are fixture data on this host.
  fileSpy = spyOn(Bun, "file").mockImplementation((path, options) => {
    if (typeof path === "number") return file(path, options);
    if (typeof path === "string" || path instanceof URL) return file(String(path) === `/proc/${process.pid}/exe` ? executable : path, options);
    return file(path, options);
  });
  if (kind === "matched") {
    await observeRunner(args, fixture.root);
    const observed = runnerProcessSchema.parse(await file(resolve(fixture.root, "observation.json")).json());
    expect(observed).toMatchObject({pid:process.pid, runner_id:RUNNER, os:"linux", binary_hash:await fileHash(file(executable))});
  } else {
    await expect(observeRunner(args, fixture.root)).rejects.toThrow(kind === "not-elf" ? "not a Linux binary" : "differs from the candidate build");
    expect(await file(resolve(fixture.root, "observation.json")).exists()).toBe(false);
  }
});
