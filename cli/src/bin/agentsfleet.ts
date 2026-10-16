#!/usr/bin/env bun
import { readBuildInfo } from "../lib/build-info.ts";
import { runCli } from "../cli.ts";

if (process.argv.length === 3 && process.argv[2] === "--build-info") {
  process.stdout.write(`${JSON.stringify(await readBuildInfo())}\n`);
  process.exit(0);
}

const exitCode = await runCli(process.argv.slice(2), {
  env: process.env,
  stdout: process.stdout,
  stderr: process.stderr,
  stdin: process.stdin,
});

process.exit(exitCode);
