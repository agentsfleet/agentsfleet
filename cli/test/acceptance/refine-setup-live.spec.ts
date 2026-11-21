/**
 * refine-setup-live — "I want this fleet to also wake every morning."
 *
 * A saved fleet is refined the way a terminal user does it: one chat message
 * to the fleet's improve conversation, a review that shows the new schedule in
 * the effective setup, an explicit apply that moves the fleet to its next
 * setup revision, and the schedule list that now carries the wake-up. No step
 * edits the draft through the API; the setup model writes the change.
 *
 * Live-only: registers real tests only when `AGENTSFLEET_ACCEPTANCE_TARGET` is
 * an https URL, matching every other live spec in this lane.
 */

import { describe, it, beforeAll, afterAll } from "bun:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { Schema } from "effect";

import { ACCEPTANCE_TARGET_ENV } from "./fixtures/constants.ts";
import { composeEnv, runFleetctl } from "./fixtures/cli.js";
import type { RunResult } from "./fixtures/cli.js";
import { assertNoSecretLeak } from "./fixtures/negatives.ts";
import { resolveAcceptanceEnv, resolveClerkSecret, resolveFixtureEmail } from "./global-setup.ts";
import { attachJwt } from "./fixtures/clerk-admin.ts";
import { readAuthContext } from "./fixtures/native-api.ts";
import { hydrateWorkspacesForToken } from "./fixtures/workspace-hydration.ts";
import { deleteFixtureFleet } from "./fixtures/teardown.ts";
import { setupModel, commandValues, fixtureDefinition, inspectDraft, saveNativeFleet } from "./fixtures/native-setup.ts";
import { AppliedSchema } from "../../src/commands/chat/protocol.ts";

const target = process.env[ACCEPTANCE_TARGET_ENV] ?? "";
const isLive = target.startsWith("https://");

const STATE_DIR_PREFIX = "agentsfleet-refine-" as const;
const FLEET_NAME = "daily-status-report";
const NO_COLOR = "1" as const;
const SETUP_TIMEOUT_MS = 180_000;
const CHAT_TIMEOUT_MS = 240_000;
const DAILY_CRON = "0 9 * * *" as const;
const REQUEST = `Add a schedule to this fleet: a cron trigger "${DAILY_CRON}" in UTC whose message is ` +
  "\"daily status\". Keep the instructions, tools and budget exactly as they are. Do not ask questions.";

/** The receipt `chat --message --json` prints before the durable messages (ChatActions.message). */
const AcceptedSchema = Schema.Struct({ kind: Schema.Literal("chat_accepted"), draft_id: Schema.String, sequence: Schema.String });
const AnswerSchema = Schema.Struct({ role: Schema.Literal("assistant"), state: Schema.Literal("completed") });

const ListedSchema = Schema.Struct({ schedules: Schema.Array(Schema.Struct({ cron: Schema.String, message: Schema.String })) });

type Cron = { type?: unknown; schedule?: unknown };
const cronTriggers = (config: unknown): Cron[] => {
  const extension = (config as Record<string, { triggers?: Cron[] }>)["x-agentsfleet"];
  return (extension?.triggers ?? []).filter(trigger => trigger.type === "cron");
};

if (!isLive) {
  describe("refine-setup-live.spec.ts", () => {
    it.skip(`requires ${ACCEPTANCE_TARGET_ENV} to be an https URL`, () => {});
  });
} else {
  describe("refine-setup-live — chat, review and apply a schedule onto a saved fleet", () => {
    let sessionJwt = "";
    let stateDir = "";
    let env: Record<string, string> = {};
    let fleetId = "";
    let savedRevision = "";
    let draftId = "";
    let reviewDigest = "";
    let model = "";

    async function run(args: ReadonlyArray<string>, timeoutMs = CHAT_TIMEOUT_MS): Promise<RunResult> {
      const result = await runFleetctl(args, { env, timeoutMs });
      assertNoSecretLeak(result, sessionJwt);
      return result;
    }

    beforeAll(async () => {
      const apiUrl = resolveAcceptanceEnv().apiUrl;
      const minted = await attachJwt(resolveClerkSecret(), { email: resolveFixtureEmail("regular") });
      sessionJwt = minted.sessionJwt;
      stateDir = await fs.mkdtemp(path.join(os.tmpdir(), STATE_DIR_PREFIX));
      env = composeEnv({ AGENTSFLEET_API_URL: apiUrl, AGENTSFLEET_STATE_DIR: stateDir, NO_COLOR });
      await hydrateWorkspacesForToken({ apiUrl, token: sessionJwt, stateDir });
      const fleet = await saveNativeFleet(env, fixtureDefinition(FLEET_NAME));
      fleetId = fleet.id;
      savedRevision = fleet.setup_revision;
      // An improve chat names its setup model, as every new chat does.
      model = await setupModel(await readAuthContext(env));
    }, SETUP_TIMEOUT_MS);

    afterAll(async () => {
      try {
        if (fleetId) await deleteFixtureFleet(env, fleetId);
      } finally {
        if (stateDir) await fs.rm(stateDir, { recursive: true, force: true });
      }
    });

    it("`chat --fleet --message` opens the fleet's improve chat and the setup model answers", async () => {
      const result = await run(["--json", "chat", "--fleet", fleetId, "--setup-revision", savedRevision, "--model", model, "--no-input", "--message", REQUEST]);
      assert.equal(result.code, 0, `the improve chat refused: ${result.stderr}`);
      const values = commandValues(result);
      const accepted = values.find(Schema.is(AcceptedSchema));
      assert.ok(accepted, `no acceptance receipt came back: ${result.stdout}`);
      assert.ok(values.some(Schema.is(AnswerSchema)), `the setup model never answered: ${result.stdout}`);
      // The fleet the draft improves is proved by apply, which names it.
      draftId = accepted.draft_id;
    }, CHAT_TIMEOUT_MS);

    it("`--review` shows the requested schedule in the effective setup", async () => {
      assert.ok(draftId, "the improve chat did not open");
      const review = await inspectDraft(env, draftId);
      const crons = cronTriggers(review.effective.definition.config);
      assert.ok(crons.some(trigger => trigger.schedule === DAILY_CRON),
        `the reviewed setup carries no ${DAILY_CRON} trigger: ${JSON.stringify(crons)}`);
      reviewDigest = review.review_digest;
    }, CHAT_TIMEOUT_MS);

    it("`--apply` moves the fleet to its next setup revision with the schedule registered", async () => {
      assert.ok(reviewDigest, "nothing was reviewed");
      const result = await run(["--json", "chat", draftId, "--no-input", "--apply", "--review-digest", reviewDigest]);
      assert.equal(result.code, 0, `apply refused: ${result.stderr}`);
      const applied = commandValues(result).find(Schema.is(AppliedSchema));
      assert.ok(applied, `apply returned no setup revision: ${result.stdout}`);
      assert.equal(applied.fleet_id, fleetId);
      assert.equal(BigInt(applied.setup_revision), BigInt(savedRevision) + 1n, "apply must append exactly one revision");
      assert.ok(applied.schedules.length > 0, "the applied setup registered no schedule");
    }, CHAT_TIMEOUT_MS);

    it("`schedule list` shows the wake-up the chat added", async () => {
      const result = await run(["fleet", "schedule", "list", fleetId, "--json"]);
      assert.equal(result.code, 0, `schedule list failed: ${result.stderr}`);
      const listed = Schema.decodeUnknownSync(ListedSchema)(JSON.parse(result.stdout.trim())).schedules;
      assert.ok(listed.some(schedule => schedule.cron === DAILY_CRON),
        `the fleet's schedules do not include ${DAILY_CRON}: ${result.stdout}`);
    });

    it("the old revision cannot be applied twice: a stale review digest is refused", async () => {
      const result = await run(["--json", "chat", draftId, "--no-input", "--apply", "--review-digest", reviewDigest]);
      assert.notEqual(result.code, 0, `a second apply of the same review must not succeed: ${result.stdout}`);
    });
  });
}
