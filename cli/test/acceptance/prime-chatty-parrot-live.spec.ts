// The Slack journey, from a terminal and a real channel: chatty-parrot adds a
// trigger for one channel, a person mentions it there, the mention wakes it,
// and its reply lands in the thread the mention started.
//
// The fleet's name and first message are
// `tests/fixtures/fleets/chatty-parrot.json`. The channel, the person who
// posts and the bot they mention are the environment's, named below. The
// mention is posted to Slack itself and Slack delivers the event; the journey
// signs nothing. It runs in the workspace that connected the Slack app.

import { describe, it, beforeAll } from "bun:test";
import assert from "node:assert/strict";
import { Schema } from "effect";

import { ACCEPTANCE_TARGET_ENV } from "./fixtures/constants.ts";
import {
  DEPLOYED_DEV_ONLY,
  connectedWorkspace,
  onDeployedDev,
  primeFleetBeside,
  requireVariables,
} from "./fixtures/prime-connected.ts";
import { entry, primeFleet as primeFixture, wake } from "./fixtures/prime-fleets.ts";
import { isLive, primeSession } from "./fixtures/prime-session.ts";
import { awaitTurn, processed, runWord } from "./fixtures/prime-turns.ts";
import { awaitReply, postMessage, type SlackMessage } from "./fixtures/slack-web.ts";

const fixture = primeFixture("chatty-parrot");
const mention = wake(fixture, "mention");

// The channel the fleet answers in, by Slack's identifier for it.
const CHANNEL_VARIABLE = "ACCEPTANCE_SLACK_CHANNEL_ID";
// A token of the person who posts the mention; it posts one message and reads one thread.
const PERSON_TOKEN_VARIABLE = "ACCEPTANCE_SLACK_USER_TOKEN";
// The Slack user the connected app answers as, which a mention has to name: `<@U…>`.
const BOT_VARIABLE = "ACCEPTANCE_SLACK_BOT_USER_ID";

const SETUP_TIMEOUT_MS = 360_000;
const STEP_TIMEOUT_MS = 300_000;
// The reply is posted once the run reports, by the daemon's own outbound delivery.
const REPLY_TIMEOUT_MS = 120_000;
const CONNECTOR_SLACK = "slack";
// `afd_ingress::slack::admit`: a mention's turn names its sender `slack:<team>:<user>`.
const SLACK_ACTOR_PREFIX = "slack:";
// `af_wire::turn::TurnKind::Chat`: a mention is a message, as a turn records how it arrived.
const TURN_KIND_CHAT = "chat";
const TRIGGERS = ["fleet", "triggers"] as const;

const TriggerPageSchema = Schema.Struct({
  items: Schema.Array(Schema.Struct({
    id: Schema.String,
    kind: Schema.String,
    source: Schema.String,
    channel: Schema.NullOr(Schema.String),
  })),
});

if (!isLive) {
  describe("prime-chatty-parrot-live.spec.ts", () => {
    it.skip(`requires ${ACCEPTANCE_TARGET_ENV} to be an https URL`, () => {});
  });
} else if (!onDeployedDev) {
  describe("prime-chatty-parrot-live.spec.ts", () => {
    it.skip(DEPLOYED_DEV_ONLY, () => {});
  });
} else {
  describe("chatty-parrot: a real Slack mention wakes the fleet, and it answers in the thread", () => {
    const session = primeSession(connectedWorkspace(CONNECTOR_SLACK));
    const word = runWord("parrot");
    let channel = "";
    let personToken = "";
    let bot = "";
    let fleetId = "";
    let fleetName = "";
    let triggerId = "";
    let posted: SlackMessage | null = null;

    const mentionTriggers = async () =>
      Schema.decodeUnknownSync(TriggerPageSchema)(await session.json([...TRIGGERS, "list", fleetId])).items
        .filter((row) => row.kind === mention.kind && row.source === mention.source);

    beforeAll(async () => {
      const given = requireVariables([CHANNEL_VARIABLE, PERSON_TOKEN_VARIABLE, BOT_VARIABLE]);
      channel = given[CHANNEL_VARIABLE];
      personToken = given[PERSON_TOKEN_VARIABLE];
      bot = given[BOT_VARIABLE];
      const created = await primeFleetBeside(session, fixture, `The channel is ${channel}.`);
      fleetId = created.fleet_id;
      fleetName = created.name;
      await awaitTurn(session, fleetId, processed((turn) => turn.turn_id === created.turn_id), "answered first turn");
    }, SETUP_TIMEOUT_MS);

    it("the Slack trigger the fleet adds shows in `fleet triggers list`, for that one channel", async () => {
      assert.ok(fleetId, "the fleet was not created");
      const mine = await mentionTriggers();
      assert.deepEqual(mine.map((row) => row.channel), [channel]);
      triggerId = entry(mine, 0, "trigger").id;
    });

    it("a person's mention in that channel wakes the fleet, in a turn Slack sent", async () => {
      assert.ok(triggerId, "the fleet added no Slack trigger");
      // Addressed by name, so the mention reaches this fleet whichever others answer in the channel.
      posted = await postMessage(personToken, channel, `<@${bot}> ${fleetName}: the word is ${word}.`);
      assert.equal(posted.bot_id, undefined,
        "Slack stored the mention as an app's message, which the daemon drops as bot_message: the token must post as a person");

      const woken = await awaitTurn(session, fleetId,
        processed((turn) => turn.actor.startsWith(SLACK_ACTOR_PREFIX)), "answered turn sent from Slack");
      assert.equal(woken.event_type, TURN_KIND_CHAT);
    }, STEP_TIMEOUT_MS);

    it("the fleet's reply is posted in the thread the mention started, repeating the word it was given", async () => {
      assert.ok(posted, "no mention was posted");
      const reply = await awaitReply(personToken, channel, posted.ts,
        (candidate) => candidate.bot_id !== undefined && (candidate.text ?? "").includes(word), REPLY_TIMEOUT_MS);
      assert.notEqual(reply.ts, posted.ts);
    }, STEP_TIMEOUT_MS);

    it("`fleet triggers delete` removes the Slack trigger", async () => {
      assert.ok(triggerId, "the fleet added no Slack trigger");
      await session.json([...TRIGGERS, "delete", fleetId, triggerId]);
      assert.deepEqual(await mentionTriggers(), []);
    });
  });
}
