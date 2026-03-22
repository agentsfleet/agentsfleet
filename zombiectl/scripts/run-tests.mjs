import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";

const testDir = path.resolve("test");
const files = fs.readdirSync(testDir)
  .filter((file) => file.endsWith(".js"))
  .map((file) => path.join("test", file))
  .sort();

const nodeTests = [];
const bunTests = [];

for (const file of files) {
  const source = fs.readFileSync(path.resolve(file), "utf8");
  if (source.includes('from "node:test"') || source.includes("from 'node:test'")) {
    nodeTests.push(file);
    continue;
  }
  if (source.includes('from "bun:test"') || source.includes("from 'bun:test'")) {
    bunTests.push(file);
  }
}

function run(command, args) {
  if (args.length === 0) return;
  const result = spawnSync(command, args, {
    stdio: "inherit",
    cwd: process.cwd(),
    env: process.env,
  });
  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

run("node", ["--test", ...nodeTests]);
run("bun", ["test", ...bunTests]);
