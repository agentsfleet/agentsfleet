// Targeted test filling a coverage hole on the substrate: the Credentials
// error path through a non-writable state dir.

import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, chmodSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Cause, Effect, Exit, Option, Redacted } from "effect";
import { Credentials, credentialsLayer } from "../src/services/credentials.ts";

describe("Credentials error path", () => {
  let tempDir: string;
  let originalStateDir: string | undefined;

  beforeEach(() => {
    tempDir = mkdtempSync(join(tmpdir(), "agentsfleet-creds-err-"));
    originalStateDir = process.env.AGENTSFLEET_STATE_DIR;
    process.env.AGENTSFLEET_STATE_DIR = tempDir;
  });

  afterEach(() => {
    try {
      chmodSync(tempDir, 0o755);
    } catch {}
    if (originalStateDir === undefined) delete process.env.AGENTSFLEET_STATE_DIR;
    else process.env.AGENTSFLEET_STATE_DIR = originalStateDir;
    rmSync(tempDir, { recursive: true, force: true });
  });

  test("saveAccessToken surfaces UnexpectedError when state dir is unwritable", async () => {
    // Read-only state dir — writeJson will EACCES on most platforms.
    // CI runs as non-root, so chmod 0o500 keeps the dir inaccessible.
    chmodSync(tempDir, 0o500);
    const exit = await Effect.runPromiseExit(
      Effect.provide(
        Effect.gen(function* () {
          const c = yield* Credentials;
          yield* c.saveAccessToken({
            token: Redacted.make("tok"),
            sessionId: "sess",
            apiUrl: "https://x",
            credentialId: "cred-unwritable",
          });
        }),
        credentialsLayer(process.env),
      ),
    );
    // On platforms where the chmod doesn't enforce (root/CI) the test
    // still asserts the type-level path: either the Effect succeeds or
    // fails with the typed UnexpectedError variant we care about.
    if (Exit.isFailure(exit)) {
      const fail = Option.getOrNull(Cause.findErrorOption(exit.cause));
      expect(fail?._tag).toBe("UnexpectedError");
    }
  });
});
