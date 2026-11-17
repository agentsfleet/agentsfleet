/**
 * memory-search-live — "what do you remember?", asked of a live fleet.
 *
 * # Why the memory is seeded by a run
 *
 * Nothing but a fleet writes its memory. The tenant plane serves `/memories`
 * for GET and one entry for DELETE (`verbs` in
 * `rustd/crates/daemon/afd_http/src/route/fleet.rs`), the CLI's memory group is
 * read-only by design (`src/program/tree/memory.command.ts`), and the only
 * write is the runner's capture of what a run stored through the
 * `memory_store` tool, pushed before the run reports (`afr_memory`). A tool
 * reaches a run only through the fleet's `tools:` allowlist
 * (`docs/architecture/capabilities.md`). So the seed is one real run: a fleet
 * allowed `memory_store` is told a fact, and the search asks for it after.
 *
 * Live-only: registers real tests only when `AGENTSFLEET_ACCEPTANCE_TARGET` is
 * an https URL, matching every other live spec in this lane.
 */

import { describe, it, beforeAll, afterAll } from "bun:test";
import assert from "node:assert/strict";
import crypto from "node:crypto";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { Schema } from "effect";

import { ACCEPTANCE_TARGET_ENV } from "./fixtures/constants.ts";
import { composeEnv, runFleetctl } from "./fixtures/cli.js";
import type { RunResult } from "./fixtures/cli.js";
import { assertNoSecretLeak, refusalCode } from "./fixtures/negatives.ts";
import { resolveAcceptanceEnv, resolveClerkSecret, resolveFixtureEmail } from "./global-setup.ts";
import { attachJwt } from "./fixtures/clerk-admin.ts";
import { hydrateWorkspacesForToken } from "./fixtures/workspace-hydration.ts";
import { saveNativeFleet } from "./fixtures/native-setup.ts";
import { waitForFleetActive } from "./fixtures/seed.ts";
import { deleteFixtureFleet } from "./fixtures/teardown.ts";
import { trailingJson } from "./fixtures/steer-envelope.ts";
import { randomUuidv7 } from "./helpers-lifecycle-with-token.ts";
import type { DefinitionSchema } from "../../src/commands/chat/protocol.ts";
import { NANOS_PER_USD } from "../../src/constants/billing.ts";
import { EXIT_CODE } from "../../src/errors/index.ts";

const target = process.env[ACCEPTANCE_TARGET_ENV] ?? "";
const isLive = target.startsWith("https://");

const STATE_DIR_PREFIX = "agentsfleet-memory-" as const;
const NO_COLOR = "1" as const;
const JSON_FLAG = "--json" as const;
const FLEET_FLAG = "--fleet" as const;
const MEMORY = ["fleet", "memory"] as const;
// The SSE round trip falls back to a ~60s poll window before rendering; the
// budget has to exceed the CLI's own internal cap.
const STEER_TIMEOUT_MS = 180_000;
const SETUP_TIMEOUT_MS = 180_000;
// `afr_tools::catalog::MEMORY_STORE`, and the category a store without one
// lands in (`af_wire::memory::PINNED_CATEGORY`).
const MEMORY_STORE_TOOL = "memory_store" as const;
const PINNED_CATEGORY = "core" as const;
const MEMORY_KEY = "release_freeze_owner" as const;
const FLEET_NAME = "release-freeze-memory";
const STATUS_PROCESSED = "processed" as const;
// af_core/src/error_code/fleet.rs: memory asked of a fleet that is not there.
const CODE_FLEET_NOT_FOUND = "UZ-MEM-002" as const;

const SteerSchema = Schema.Struct({ event_id: Schema.String, status: Schema.String });
const EntrySchema = Schema.Struct({
  key: Schema.String,
  content: Schema.String,
  category: Schema.String,
  writer_fleet_id: Schema.String,
});
const PageSchema = Schema.Struct({ items: Schema.Array(EntrySchema), total: Schema.Number });

// A word nothing but this run's message contains, so a match can only be the
// stored fact.
const freshWord = (): string => `harbour${crypto.randomBytes(4).toString("hex")}`;

function rememberingFleet(name: string): typeof DefinitionSchema.Type {
  return {
    schema_version: 1,
    description: "Remembers the fact it is told",
    instructions_md: `When a message arrives, call the ${MEMORY_STORE_TOOL} tool exactly once with key ` +
      `"${MEMORY_KEY}", category "${PINNED_CATEGORY}" and content equal to the message text, ` +
      "then reply STORED and stop. Call no other tool.",
    config: { name, "x-agentsfleet": { triggers: [], tools: [MEMORY_STORE_TOOL], credentials: [],
      budget: { daily_nanos: String(NANOS_PER_USD) } } },
  };
}

if (!isLive) {
  describe("memory-search-live.spec.ts", () => {
    it.skip(`requires ${ACCEPTANCE_TARGET_ENV} to be an https URL`, () => {});
  });
} else {
  describe("memory-search-live — a fleet remembers what it was told, and search finds it", () => {
    let sessionJwt = "";
    let stateDir = "";
    let env: Record<string, string> = {};
    let fleetId = "";
    const word = freshWord();
    const fact = `The release freeze owner this week is the ${word} rota.`;

    async function run(args: ReadonlyArray<string>, timeoutMs?: number): Promise<RunResult> {
      const result = await runFleetctl(args, timeoutMs === undefined ? { env } : { env, timeoutMs });
      assertNoSecretLeak(result, sessionJwt);
      return result;
    }

    async function memoryPage(args: ReadonlyArray<string>): Promise<typeof PageSchema.Type> {
      const result = await run([...MEMORY, ...args, FLEET_FLAG, fleetId, JSON_FLAG]);
      assert.equal(result.code, 0, `memory ${args[0]} failed: ${result.stderr}`);
      return Schema.decodeUnknownSync(PageSchema)(JSON.parse(result.stdout.trim()));
    }

    beforeAll(async () => {
      const apiUrl = resolveAcceptanceEnv().apiUrl;
      const minted = await attachJwt(resolveClerkSecret(), { email: resolveFixtureEmail("regular") });
      sessionJwt = minted.sessionJwt;
      stateDir = await fs.mkdtemp(path.join(os.tmpdir(), STATE_DIR_PREFIX));
      env = composeEnv({ AGENTSFLEET_API_URL: apiUrl, AGENTSFLEET_STATE_DIR: stateDir, NO_COLOR });
      await hydrateWorkspacesForToken({ apiUrl, token: sessionJwt, stateDir });
      fleetId = (await saveNativeFleet(env, rememberingFleet(FLEET_NAME))).id;
      await waitForFleetActive(env, fleetId, Date.now() + SETUP_TIMEOUT_MS);
    }, SETUP_TIMEOUT_MS);

    afterAll(async () => {
      try {
        if (fleetId) await deleteFixtureFleet(env, fleetId);
      } finally {
        if (stateDir) await fs.rm(stateDir, { recursive: true, force: true });
      }
    });

    it("a run told one fact finishes processed", async () => {
      assert.ok(fleetId, "the fleet was not saved in beforeAll");
      const steered = await run(["fleet", "steer", fleetId, fact, JSON_FLAG], STEER_TIMEOUT_MS);
      assert.equal(steered.code, 0, `the run did not finish: stdout=${steered.stdout} stderr=${steered.stderr}`);
      const envelope = Schema.decodeUnknownSync(SteerSchema)(trailingJson(steered.stdout));
      assert.equal(envelope.status, STATUS_PROCESSED, `expected ${STATUS_PROCESSED}: ${JSON.stringify(envelope)}`);
    }, STEER_TIMEOUT_MS);

    it("`memory search` finds the fact by a word inside it", async () => {
      assert.ok(fleetId, "the fleet was not saved in beforeAll");
      const found = await memoryPage(["search", word]);
      const entry = found.items.find((item) => item.key === MEMORY_KEY);
      if (entry === undefined) {
        const events = await run(["fleet", "events", fleetId, JSON_FLAG]);
        assert.fail(`search for ${word} found no ${MEMORY_KEY}: ${JSON.stringify(found)}; events=${events.stdout}`);
      }
      assert.ok(entry.content.includes(word), `the stored fact lost the word searched for: ${entry.content}`);
      assert.equal(entry.writer_fleet_id, fleetId, "the entry must be the one this fleet's run stored");
      assert.ok(found.total >= 1);
    });

    it("`memory list` shows the stored entry under its key and category", async () => {
      assert.ok(fleetId, "the fleet was not saved in beforeAll");
      const entry = (await memoryPage(["list"])).items.find((item) => item.key === MEMORY_KEY);
      assert.ok(entry, `memory list does not show ${MEMORY_KEY}`);
      assert.equal(entry.category, PINNED_CATEGORY);
    });

    it("`memory search` for a word the fleet was never told answers an empty page, not an error", async () => {
      assert.ok(fleetId, "the fleet was not saved in beforeAll");
      const missed = await memoryPage(["search", freshWord()]);
      assert.deepEqual(missed.items, []);
      assert.equal(missed.total, 0);
    });

    it("`memory search` on a fleet the workspace does not hold is refused as unknown", async () => {
      const refused = await run([...MEMORY, "search", word, FLEET_FLAG, randomUuidv7(), JSON_FLAG]);
      assert.equal(refused.code, EXIT_CODE.ServerError, `an unknown fleet answered: ${refused.stdout}`);
      assert.equal(refusalCode(refused), CODE_FLEET_NOT_FOUND);
    });
  });
}
