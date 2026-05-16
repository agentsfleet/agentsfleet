#!/usr/bin/env node
import { runCli } from "../cli.ts";

const exitCode = await runCli(process.argv.slice(2), {
  env: process.env,
  stdout: process.stdout,
  stderr: process.stderr,
  stdin: process.stdin,
});

process.exit(exitCode);
