#!/usr/bin/env bun
import { readBuildInfo } from "../lib/build-info.ts";
import { exitFlushed } from "../lib/exit-flushed.ts";
import { runCli } from "../cli.ts";

const OUTPUT_STREAMS = [process.stdout, process.stderr] as const;

if (process.argv.length === 3 && process.argv[2] === "--build-info") {
  process.stdout.write(`${JSON.stringify(await readBuildInfo())}\n`);
  await exitFlushed(0, OUTPUT_STREAMS);
}

const exitCode = await runCli(process.argv.slice(2), {
  env: process.env,
  stdout: process.stdout,
  stderr: process.stderr,
  stdin: process.stdin,
});

await exitFlushed(exitCode, OUTPUT_STREAMS);
