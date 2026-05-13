import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";

const testDir = path.resolve("test");

function walk(dir) {
  const out = [];
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      out.push(...walk(full));
    } else if (entry.isFile() && entry.name.endsWith(".js")) {
      out.push(full);
    }
  }
  return out;
}

const files = walk(testDir).sort();

const nodeTests = [];
const bunTests = [];

for (const file of files) {
  const source = fs.readFileSync(file, "utf8");
  if (source.includes('from "node:test"') || source.includes("from 'node:test'")) {
    nodeTests.push(path.relative(process.cwd(), file));
    continue;
  }
  if (source.includes('from "bun:test"') || source.includes("from 'bun:test'")) {
    bunTests.push(path.relative(process.cwd(), file));
  }
}

function run(command, baseArgs, targets) {
  if (targets.length === 0) return;
  const result = spawnSync(command, [...baseArgs, ...targets], {
    stdio: "inherit",
    cwd: process.cwd(),
    env: process.env,
  });
  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

run("node", ["--test"], nodeTests);
run("bun", ["test"], bunTests);
