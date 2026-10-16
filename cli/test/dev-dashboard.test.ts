import {afterEach, beforeEach, describe, expect, test} from "bun:test";
import {chmod, mkdtemp, rm} from "node:fs/promises";
import {tmpdir} from "node:os";
import {resolve} from "node:path";
import {deployDevDashboard, prepareDevDashboard} from "../../scripts/dev-dashboard.ts";
import {runDevDashboard} from "../../scripts/deploy-dev-dashboard.ts";
import {WORKTREE_ROOT, sourceRevision} from "../../scripts/build-source.ts";

const PROJECT = {id: "prj_fixture", accountId: "team_fixture", rootDirectory: "ui/packages/app"};
const DEPLOYMENT = "https://revision-fixture.vercel.app/";
let root: string;
let server: ReturnType<typeof Bun.serve>;
let projectResponse: () => Response;
let env: NodeJS.ProcessEnv;

beforeEach(async () => {
  root = await mkdtemp(resolve(tmpdir(), "dev-dashboard-test-"));
  env = {...Bun.env, VERCEL_TOKEN: "fixture-only", TRACE: resolve(root, "trace.jsonl")};
  const gitEnv = Object.fromEntries(Object.entries(env).filter(([key]) => !key.startsWith("GIT_")));
  if (Bun.spawnSync(["git", "clone", "--shared", "--no-checkout", WORKTREE_ROOT, root], {env: gitEnv, stderr: "pipe"}).exitCode) {
    throw new Error("isolated dashboard fixture could not clone");
  }
  projectResponse = () => Response.json(PROJECT);
  server = Bun.serve({hostname: "127.0.0.1", port: 0, fetch: () => projectResponse()});
  await Bun.write(resolve(root, "vercel"), `#!${process.execPath}
import {appendFile} from "node:fs/promises";
const args = Bun.argv.slice(2);
await appendFile(Bun.env.TRACE, JSON.stringify(args) + "\\n");
if (Bun.env.FAIL_STEP === args[0]) process.exit(7);
if (Bun.env.HANG_STEP === args[0]) await Bun.sleep(60000);
if (args[0] === "pull") await Bun.write(".vercel/project.json", (Bun.env.LOCAL_PROJECT_FILE ? await Bun.file(Bun.env.LOCAL_PROJECT_FILE).text() : undefined) ?? JSON.stringify({projectId:"prj_fixture",orgId:"team_fixture",settings:{framework:"nextjs",installCommand:"bun install"}}));
if (args[0] === "deploy") process.stdout.write((Bun.env.DEPLOYMENT_FILE ? await Bun.file(Bun.env.DEPLOYMENT_FILE).text() : Bun.env.DEPLOYMENT) ?? ${JSON.stringify(DEPLOYMENT)});
`);
  await chmod(resolve(root, "vercel"), 0o700);
});
afterEach(async () => { server.stop(true); await rm(root, {recursive: true, force: true}); });
function options() { return {root, apiUrl: server.url.href, executable: resolve(root, "vercel"), env}; }
async function trace(): Promise<string[][]> {
  const file = Bun.file(env.TRACE!);
  return await file.exists() ? (await file.text()).trim().split("\n").map(line => JSON.parse(line)) : [];
}

describe("development dashboard preparation", () => {
  test("dispatches explicit modes and refuses unknown commands", async () => {
    await expect(runDevDashboard(undefined, options())).rejects.toThrow("Expected development dashboard");
    await runDevDashboard("prepare", options());
    await runDevDashboard("deploy", options());
    expect((await trace()).map(args => args[0])).toEqual(["pull", "deploy", "alias"]);
  });
  test("pins the actual project and custom development environment without reinstalling with secrets", async () => {
    await prepareDevDashboard(options());
    expect(await trace()).toEqual([["pull", "--yes", "--environment=development-agentsfleet"]]);
    expect(await Bun.file(resolve(root, ".vercel/project.json")).json()).toEqual({projectId:PROJECT.id, orgId:PROJECT.accountId,
      settings:{framework:"nextjs",installCommand:"true"}});
    expect(await Bun.file(resolve(root, ".vercel/project.json")).text()).not.toContain(env.VERCEL_TOKEN!);
  });
  test("refuses missing credentials before contacting Vercel", async () => {
    delete env.VERCEL_TOKEN;
    await expect(prepareDevDashboard(options())).rejects.toThrow("VERCEL_TOKEN is required");
    expect(await trace()).toEqual([]);
  });
  test.each([403, 500])("refuses project read HTTP %i", async (status) => {
    projectResponse = () => new Response("failed", {status});
    await expect(prepareDevDashboard(options())).rejects.toThrow(`project read failed: ${status}`);
    expect(await trace()).toEqual([]);
  });
  test.each(["{", JSON.stringify({...PROJECT, rootDirectory:"ui/packages/website"}), "x".repeat(1048577)])("refuses invalid or oversized project metadata", async (body) => {
    projectResponse = () => new Response(body);
    await expect(prepareDevDashboard(options())).rejects.toThrow();
    expect(await trace()).toEqual([]);
  });
  test.each(["{}", "x".repeat(1048577)])("refuses invalid or oversized pulled settings", async (body) => {
    env.LOCAL_PROJECT_FILE = resolve(root, "pulled.json");
    await Bun.write(env.LOCAL_PROJECT_FILE, body);
    await expect(prepareDevDashboard(options())).rejects.toThrow();
  });
  test("propagates pull failure", async () => {
    env.FAIL_STEP = "pull";
    await expect(prepareDevDashboard(options())).rejects.toThrow("Vercel pull failed");
  });
});

describe("development dashboard deployment", () => {
  test("deploys the prebuilt revision to the development target and aliases only that URL", async () => {
    expect(await deployDevDashboard(options())).toBe(DEPLOYMENT);
    expect(await trace()).toEqual([
      ["deploy","--prebuilt","--yes","--target=development-agentsfleet","--meta",`githubCommitSha=${sourceRevision(root)}`,"--meta","githubCommitRef=main"],
      ["alias","set",DEPLOYMENT,"app-dev.agentsfleet.net"],
    ]);
  });
  test.each(["http://revision-fixture.vercel.app/", "https://vercel.app.evil.test/", "https://u:p@fixture.vercel.app/", "https://fixture.vercel.app/?x=1", "https://fixture.vercel.app/#x"])("refuses unsafe alias destination %s", async (url) => {
    env.DEPLOYMENT = url;
    await expect(deployDevDashboard(options())).rejects.toThrow("unexpected development deployment URL");
    expect(await trace()).toHaveLength(1);
  });
  test.each(["deploy", "alias"])("propagates %s failure", async (step) => {
    env.FAIL_STEP = step;
    await expect(deployDevDashboard(options())).rejects.toThrow(`Vercel ${step} failed`);
  });
  test("bounds command output", async () => {
    env.DEPLOYMENT_FILE = resolve(root, "output.txt");
    await Bun.write(env.DEPLOYMENT_FILE, "x".repeat(1048577));
    await expect(deployDevDashboard(options())).rejects.toThrow("exceeds its bound");
  });
  test("kills a command that exceeds its deadline", async () => {
    env.HANG_STEP = "deploy";
    await expect(deployDevDashboard({...options(), timeoutMs:100})).rejects.toThrow();
    expect(await trace()).toHaveLength(1);
  });
});
