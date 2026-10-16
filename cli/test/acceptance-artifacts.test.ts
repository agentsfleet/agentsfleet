import {afterEach, beforeEach, describe, expect, test} from "bun:test";
import {mkdtemp, rm} from "node:fs/promises";
import {tmpdir} from "node:os";
import {resolve} from "node:path";
import {ACCEPTANCE_ARTIFACTS, buildAcceptanceComponent, readAcceptanceComponents, type BuildComponent} from "../../scripts/acceptance-artifacts.ts";
import {WORKTREE_ROOT, sourceIdentity, sourceRevision} from "../../scripts/build-source.ts";
import {BUILD_COMPONENTS} from "../../scripts/build-manifest.ts";

let root: string;
beforeEach(async () => {
  root = await mkdtemp(resolve(tmpdir(), "acceptance-build-test-"));
  const env = Object.fromEntries(Object.entries(Bun.env).filter(([key]) => !key.startsWith("GIT_")));
  const clone = Bun.spawnSync(["git", "clone", "--shared", "--no-checkout", WORKTREE_ROOT, root], {env, stderr:"pipe"});
  if (clone.exitCode) throw new Error("Cannot create isolated build fixture");
  await Bun.write(resolve(root, ".gitignore"), "dist/\ncli/dist/\nui/packages/app/.next/\n");
  await Bun.write(resolve(root, "cli/input.ts"), "export const input = 1;\n");
});
afterEach(async () => {await rm(root, {recursive:true, force:true});});

function command(component: BuildComponent, after = ""): string[] {
  const artifact = ACCEPTANCE_ARTIFACTS[component];
  const path = component === "cli" || component === "app" ? `${artifact}/output` : artifact;
  return ["bun", "-e", `await Bun.write(${JSON.stringify(path)}, Bun.env.AGENTSFLEET_BUILD_ID);${after}`];
}
async function candidate(): Promise<void> {
  for (const component of BUILD_COMPONENTS) await buildAcceptanceComponent(component, command(component), {root});
}
function receiptPath(component: BuildComponent): string {return resolve(root, "dist/acceptance-components", `${component}.json`);}

describe("component build provenance", () => {
  test("passes workflow heredoc commands through to the native build shell", async () => {
    const module = resolve(WORKTREE_ROOT, "scripts/acceptance-artifacts.ts");
    const script = `import {buildAcceptanceComponent} from ${JSON.stringify(module)}; await buildAcceptanceComponent("runner", ["bash", "-euo", "pipefail"], {root: process.argv[1]});`;
    const child = Bun.spawn(["bun", "-e", script, root], {stdin: "pipe", stdout: "pipe", stderr: "pipe"});
    child.stdin.write('mkdir -p dist\nprintf "%s" "$AGENTSFLEET_BUILD_ID" > dist/agentsfleet-runner-linux-amd64\n');
    child.stdin.end();
    const error = await new Response(child.stderr).text();
    expect(await child.exited, error).toBe(0);
    const receipt = await Bun.file(receiptPath("runner")).json();
    expect(await Bun.file(resolve(root, ACCEPTANCE_ARTIFACTS.runner)).text()).toBe(receipt.source_id);
  });
  test("records the source identity injected into a successful compiler", async () => {
    const receipt = await buildAcceptanceComponent("daemon", command("daemon"), {root});
    expect(receipt.revision).toBe(sourceRevision(root));
    expect(receipt.source_id).toBe(await sourceIdentity(root));
    expect(await Bun.file(resolve(root, ACCEPTANCE_ARTIFACTS.daemon)).text()).toBe(receipt.source_id);
    expect(await Bun.file(receiptPath("daemon")).json()).toEqual(receipt);
  });
  test("failed compilation cannot leave a usable receipt", async () => {
    await expect(buildAcceptanceComponent("runner", command("runner", "process.exit(3)"), {root})).rejects.toThrow("build failed");
    expect(await Bun.file(receiptPath("runner")).exists()).toBe(false);
  });
  test("a failed rebuild removes a previous success receipt", async () => {
    await buildAcceptanceComponent("runner", command("runner"), {root});
    await expect(buildAcceptanceComponent("runner", ["bun", "-e", "process.exit(3)"], {root})).rejects.toThrow("build failed");
    expect(await Bun.file(receiptPath("runner")).exists()).toBe(false);
  });
  test("source changes during compilation invalidate its output", async () => {
    await expect(buildAcceptanceComponent("cli", command("cli", 'await Bun.write("cli/input.ts", "changed")'), {root})).rejects.toThrow("Sources or revision changed");
    expect(await Bun.file(receiptPath("cli")).exists()).toBe(false);
  });
  test("success without an output is not build evidence", async () => {
    await expect(buildAcceptanceComponent("app", ["bun", "-e", "process.exit(0)"], {root})).rejects.toThrow();
  });
  test("requires a build command", async () => {
    await expect(buildAcceptanceComponent("app", [], {root})).rejects.toThrow("command is required");
  });
  test("terminates a compiler exceeding its deadline", async () => {
    await expect(buildAcceptanceComponent("runner", ["bun", "-e", "await Bun.sleep(60000)"], {root, timeoutMs:100})).rejects.toThrow();
    expect(await Bun.file(receiptPath("runner")).exists()).toBe(false);
  });
});

describe("candidate assembly", () => {
  test("retains all four successful outputs without rebuilding", async () => {
    await candidate();
    const receipts = await readAcceptanceComponents(root);
    expect(Object.keys(receipts)).toEqual([...BUILD_COMPONENTS]);
    expect(new Set(Object.values(receipts).map(receipt => receipt.source_id)).size).toBe(1);
  });
  for (const component of BUILD_COMPONENTS) {
    test(`refuses changed ${component} output`, async () => {
      await candidate();
      const path = ACCEPTANCE_ARTIFACTS[component];
      await Bun.write(resolve(root, component === "cli" || component === "app" ? `${path}/output` : path), "tampered");
      await expect(readAcceptanceComponents(root)).rejects.toThrow("artifact differs");
    });
  }
  for (const [field, value] of [["revision", "0".repeat(40)], ["source_id", "0".repeat(64)], ["component", "cli"]] as const) {
    test(`refuses a foreign receipt ${field}`, async () => {
      await candidate();
      const file = Bun.file(receiptPath("daemon"));
      await Bun.write(file, JSON.stringify({...await file.json(), [field]:value}));
      await expect(readAcceptanceComponents(root)).rejects.toThrow("receipt differs");
    });
  }
  test("refuses missing receipts", async () => {await expect(readAcceptanceComponents(root)).rejects.toThrow();});
  test("bounds receipt reads before decoding", async () => {
    await candidate();
    await Bun.write(receiptPath("daemon"), "x".repeat(65537));
    await expect(readAcceptanceComponents(root)).rejects.toThrow("exceeds its bound");
  });
});
