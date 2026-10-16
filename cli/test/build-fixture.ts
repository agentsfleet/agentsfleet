import {chmod, mkdtemp, rm} from "node:fs/promises";
import {tmpdir} from "node:os";
import {resolve} from "node:path";
import {WORKTREE_ROOT} from "../../scripts/build-source.ts";

/** Isolated repository and physical child programs; never writes shipping build outputs. */
export async function buildFixture(): Promise<{root: string; cleanup: () => Promise<void>}> {
  const root = await mkdtemp(resolve(tmpdir(), "acceptance-compiler-test-"));
  const env = Object.fromEntries(Object.entries(Bun.env).filter(([key]) => !key.startsWith("GIT_")));
  const clone = Bun.spawnSync(["git", "clone", "--shared", "--no-checkout", WORKTREE_ROOT, root], {env, stderr: "pipe"});
  if (clone.exitCode) throw new Error("Cannot create isolated compiler fixture");
  await Bun.write(resolve(root, ".gitignore"), "dist/\ncli/dist/\nui/packages/app/.next/\n");
  await Bun.write(resolve(root, "VERSION"), "1.2.3\n");
  await Bun.write(resolve(root, "rustd/crates/afd_fleet_runtime/src/definition/mod.rs"),
    'pub const DEFINITION_SCHEMA_VERSION: u8 = 7;\npub const POLICY_SEMANTICS: &str = "review-v7";');
  await Bun.write(resolve(root, "rustd/crates/afd_db/src/migration.rs"), 'migration!("956_model_inference");');
  await Bun.write(resolve(root, "rustd/Cargo.lock"), '[[package]]\nname = "tiktoken-rs"\nversion = "0.9.9"\n');
  for (const path of ["afd_inference/src/lib.rs", "afd_wire/src/inference.rs", "afd_credential/src/lib.rs",
    "afd_authoring/src/model/provider/mod.rs", "afr_providers/src/lib.rs"]) {
    await Bun.write(resolve(root, "rustd/crates", path), `source for ${path}`);
  }
  await Bun.write(resolve(root, "schema/956_model_inference.sql"), "fixture migration");
  for (const [directory, output] of [["cli", "dist/compiled"], ["ui/packages/app", ".next/compiled"]] as const) {
    await Bun.write(resolve(root, directory, "package.json"), JSON.stringify({scripts: {build: "bun compile.ts"}}));
    await Bun.write(resolve(root, directory, "compile.ts"),
      `if (Bun.env.ACCEPTANCE_FIXTURE_MODE === "package-failure") process.exit(7); await Bun.write(${JSON.stringify(output)}, Bun.env.AGENTSFLEET_BUILD_ID);`);
  }
  await Bun.write(resolve(root, "cli/compile.ts"),
    'if (Bun.env.ACCEPTANCE_FIXTURE_MODE === "package-failure") process.exit(7); await Bun.write("dist/bin/agentsfleet.js", `console.log(JSON.stringify(${JSON.stringify({build_id:Bun.env.AGENTSFLEET_BUILD_ID})}))`);');
  const cargo = resolve(root, "bin/cargo");
  await Bun.write(cargo, `#!${process.execPath}\n${CARGO_FIXTURE}`);
  await chmod(cargo, 0o755);
  return {root, cleanup: () => rm(root, {recursive: true, force: true})};
}

const CARGO_FIXTURE = `
import {resolve} from "node:path";
const root = resolve(process.cwd(), "..");
const mode = Bun.env.ACCEPTANCE_FIXTURE_MODE;
const binary = Bun.argv[Bun.argv.indexOf("--bin") + 1];
if (mode === "native-failure") process.exit(3);
if (mode === "missing-artifact") process.exit(0);
if (mode === "invalid-record") { console.log("not JSON"); await Bun.sleep(60000); }
const output = resolve(root, "dist", binary);
const identity = new TextEncoder().encode(Bun.env.AGENTSFLEET_BUILD_ID);
await Bun.write(output, binary === "agentsfleet-runner" ? new Uint8Array([0x7f,0x45,0x4c,0x46,...identity]) : identity);
if (mode === "source-change") await Bun.write(resolve(root, "cli/changed.ts"), "changed source");
if (mode === "revision-change") {
  const child = Bun.spawnSync(["git", "update-ref", "HEAD", "HEAD~1"], {cwd: root});
  if (child.exitCode) process.exit(9);
}
console.log(JSON.stringify({reason:"build-script-executed"}));
console.log(JSON.stringify({reason:"compiler-artifact", target:{name:"unrelated-library"}, executable:null}));
console.log(JSON.stringify({reason:"compiler-artifact", target:{name:binary}, executable:output}));
`;
