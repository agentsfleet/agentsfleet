import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { loadEnvConfig } from "@next/env";
import {
  captureRouteBuildFingerprint,
  writeRouteBuildProvenance,
} from "./route-build-provenance";

const COMMAND = {
  capture: "capture",
  commit: "commit",
} as const;
const appRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");

async function main(): Promise<void> {
  loadEnvConfig(appRoot);
  const [command, expectedFingerprint] = process.argv.slice(2);
  if (command === COMMAND.capture) {
    process.stdout.write(await captureRouteBuildFingerprint(appRoot));
    return;
  }
  if (command === COMMAND.commit && expectedFingerprint) {
    await writeRouteBuildProvenance(appRoot, expectedFingerprint);
    return;
  }
  throw new Error("usage: write-route-build-provenance.ts capture|commit <fingerprint>");
}

main().catch((error: unknown) => {
  const message = error instanceof Error ? error.message : String(error);
  process.stderr.write(`${message}\n`);
  process.exitCode = 1;
});
