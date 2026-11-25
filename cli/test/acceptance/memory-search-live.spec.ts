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
 * `memory_store` tool, pushed before the run reports (`afr_memory`). Every run
 * is offered the tool, which it calls only when the person's newest message
 * asks. So the seed is one real run: a message asks the fleet to remember a
 * fact, and the search asks for it after.
 *
 * The fleet is a night-owl, and the fact is what its fixture says it is told
 * to pin (`remembers` in `tests/fixtures/fleets/night-owl.json`), with a word
 * of this run's after it.
 *
 * Live-only: registers real tests only when `AGENTSFLEET_ACCEPTANCE_TARGET` is
 * an https URL, matching every other live spec in this lane.
 */

import { describe, it, beforeAll } from "bun:test";
import assert from "node:assert/strict";
import { Schema } from "effect";

import { ACCEPTANCE_TARGET_ENV } from "./fixtures/constants.ts";
import { refusalCode } from "./fixtures/negatives.ts";
import { entry, primeFleet as primeFixture } from "./fixtures/prime-fleets.ts";
import { isLive, ownWorkspace, primeFleet, primeSession } from "./fixtures/prime-session.ts";
import { awaitTurn, processed, runWord } from "./fixtures/prime-turns.ts";
import { trailingJson } from "./fixtures/steer-envelope.ts";
import { randomUuidv7 } from "./helpers-lifecycle-with-token.ts";
import { EXIT_CODE } from "../../src/errors/index.ts";

const fixture = primeFixture("night-owl");
const guidance = entry(fixture.remembers, 0, "standing guidance");
// This file's own workspace: the wake-ups journey creates a night-owl too, and
// a workspace holds one fleet of a name.
const WORKSPACE = "fixture-cli-memory";

const JSON_FLAG = "--json" as const;
const FLEET_FLAG = "--fleet" as const;
const MEMORY = ["fleet", "memory"] as const;
// The SSE round trip falls back to a ~60s poll window before rendering; the
// budget has to exceed the CLI's own internal cap.
const STEER_TIMEOUT_MS = 180_000;
const SETUP_TIMEOUT_MS = 360_000;
// `afr_tools::catalog::MEMORY_STORE`, and the category the message names: only
// an explicit `core` is pinned into every later prompt.
const MEMORY_STORE_TOOL = "memory_store" as const;
const PINNED_CATEGORY = "core" as const;
const MEMORY_KEY = "standing_guidance" as const;
const STATUS_PROCESSED = "processed" as const;
// af_core/src/error_code/fleet.rs: memory asked of a fleet that is not there.
const CODE_FLEET_NOT_FOUND = "UZ-MEM-002" as const;

const SteerSchema = Schema.Struct({ turn_id: Schema.String, status: Schema.String });
const EntrySchema = Schema.Struct({
  key: Schema.String,
  content: Schema.String,
  category: Schema.String,
  writer_fleet_id: Schema.String,
});
const PageSchema = Schema.Struct({ items: Schema.Array(EntrySchema), total: Schema.Number });

// A word nothing but this run's message contains, so a match can only be the
// stored fact.
const freshWord = (): string => runWord("harbour");

// The message that asks for the store, carrying the fact as the content.
function rememberThis(fact: string): string {
  return `Remember this: call the ${MEMORY_STORE_TOOL} tool exactly once with key "${MEMORY_KEY}", ` +
    `category "${PINNED_CATEGORY}" and content "${fact}", then reply STORED and stop. Call no other tool.`;
}

if (!isLive) {
  describe("memory-search-live.spec.ts", () => {
    it.skip(`requires ${ACCEPTANCE_TARGET_ENV} to be an https URL`, () => {});
  });
} else {
  describe("memory-search-live — a fleet remembers what it was told, and search finds it", () => {
    const session = primeSession(ownWorkspace(WORKSPACE));
    const run = session.run;
    let fleetId = "";
    const word = freshWord();
    const fact = `${guidance} The word for this run is ${word}.`;

    async function memoryPage(args: ReadonlyArray<string>): Promise<typeof PageSchema.Type> {
      const result = await run([...MEMORY, ...args, FLEET_FLAG, fleetId, JSON_FLAG]);
      assert.equal(result.code, 0, `memory ${args[0]} failed: ${result.stderr}`);
      return Schema.decodeUnknownSync(PageSchema)(JSON.parse(result.stdout.trim()));
    }

    beforeAll(async () => {
      // The first message asks for nothing the fleet would store, and its turn
      // is waited out so the message that asks starts a run of its own.
      const created = await primeFleet(session, fixture);
      fleetId = created.fleet_id;
      await awaitTurn(session, fleetId, processed((turn) => turn.turn_id === created.turn_id), "answered first turn");
    }, SETUP_TIMEOUT_MS);

    it("a run asked to remember one fact finishes processed", async () => {
      assert.ok(fleetId, "the fleet was not created in beforeAll");
      const steered = await run(["fleet", "steer", fleetId, rememberThis(fact), JSON_FLAG], STEER_TIMEOUT_MS);
      assert.equal(steered.code, 0, `the run did not finish: stdout=${steered.stdout} stderr=${steered.stderr}`);
      const envelope = Schema.decodeUnknownSync(SteerSchema)(trailingJson(steered.stdout));
      assert.equal(envelope.status, STATUS_PROCESSED, `expected ${STATUS_PROCESSED}: ${JSON.stringify(envelope)}`);
    }, STEER_TIMEOUT_MS);

    it("`memory search` finds the fact by a word inside it", async () => {
      assert.ok(fleetId, "the fleet was not created in beforeAll");
      const found = await memoryPage(["search", word]);
      const entry = found.items.find((item) => item.key === MEMORY_KEY);
      if (entry === undefined) {
        const events = await run(["fleet", "turns", fleetId, JSON_FLAG]);
        assert.fail(`search for ${word} found no ${MEMORY_KEY}: ${JSON.stringify(found)}; events=${events.stdout}`);
      }
      assert.ok(entry.content.includes(word), `the stored fact lost the word searched for: ${entry.content}`);
      assert.equal(entry.writer_fleet_id, fleetId, "the entry must be the one this fleet's run stored");
      assert.ok(found.total >= 1);
    });

    it("`memory list` shows the stored entry under its key and category", async () => {
      assert.ok(fleetId, "the fleet was not created in beforeAll");
      const entry = (await memoryPage(["list"])).items.find((item) => item.key === MEMORY_KEY);
      assert.ok(entry, `memory list does not show ${MEMORY_KEY}`);
      assert.equal(entry.category, PINNED_CATEGORY);
    });

    it("`memory search` for a word the fleet was never told answers an empty page, not an error", async () => {
      assert.ok(fleetId, "the fleet was not created in beforeAll");
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
