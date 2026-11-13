import {execFile} from "node:child_process";
import {createHash} from "node:crypto";
import {readFile} from "node:fs/promises";
import {resolve} from "node:path";
import {promisify} from "node:util";

const execute = promisify(execFile);

/** Shared by sample and authenticated captures; source changes refuse evidence. */
export async function visualProvenance() {
  const root = (await execute("git", ["rev-parse", "--show-toplevel"])).stdout.trim();
  const git = async (...args: string[]) => (await execute("git", args, {cwd: root, maxBuffer: 5_000_000})).stdout;
  const revision = (await git("rev-parse", "HEAD")).trim();
  const status = await git("status", "--porcelain");
  if (status !== "") throw new Error("Visual evidence requires a clean candidate checkout.");
  const tracked = (await git("ls-files", "-z", "ui", "package.json", "bun.lock")).split("\0");
  const names = tracked.filter(name => name && !/\.(test|spec)\.[tj]sx?$/.test(name) && !name.endsWith("/next-env.d.ts")).sort();
  const files = [];
  for (const name of names) files.push({name, sha256: createHash("sha256").update(await readFile(resolve(root, name))).digest("hex")});
  return {revision, sourceTreeSha256: createHash("sha256").update(JSON.stringify(files)).digest("hex"),
    captureTimeUtc: new Date().toISOString(), gitStatusPorcelain: status};
}
