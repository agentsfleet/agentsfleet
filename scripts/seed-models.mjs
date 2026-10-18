#!/usr/bin/env bun
// Review with make seed-models; apply with make seed-models APPLY=1.
import { runSeedCommand } from "./model-catalogue/command.ts";

process.exitCode = await runSeedCommand({ args: Bun.argv.slice(2), databaseUrl: Bun.env.DATABASE_URL });
