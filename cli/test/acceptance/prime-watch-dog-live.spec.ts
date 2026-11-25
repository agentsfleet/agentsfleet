// The GitHub journey, from a terminal: watch-dog asks to read one repository,
// a person approves it once, the fleet subscribes to that repository's pull
// requests, and a pull request a person opens there wakes it.
//
// The fleet's name, first message, repository and the access it asks for are
// `tests/fixtures/fleets/watch-dog.json`. It runs in the workspace that
// connected the platform's GitHub App.
//
// The daemon drops GitHub events on a fleet's own `agentsfleet-repair/`
// branches (afd_api_ingress/src/handler/webhook/github/mod.rs), so the event
// has to come from outside: the journey opens the pull request as a test
// account, with that account's fine-grained token for the repository in
// `ACCEPTANCE_GITHUB_TOKEN`, and closes it and deletes its branch with the same
// token once the wake is counted. The fleet only reads.

import { describe, it, beforeAll } from "bun:test";
import assert from "node:assert/strict";
import { Schema } from "effect";

import { ACCEPTANCE_TARGET_ENV } from "./fixtures/constants.ts";
import { GITHUB_REQUEST_TIMEOUT_MS, callGithub } from "./fixtures/github-rest.ts";
import { GRANT_KIND, GRANT_STATUS, decide, grantFor, raisedCard, resumedTurn, type Asked } from "./fixtures/grant-ops.ts";
import { DEPLOYED_DEV_ONLY, connectedWorkspace, onDeployedDev, primeFleetBeside, requireVariables } from "./fixtures/prime-connected.ts";
import { entry, field, primeFleet as primeFixture, wake } from "./fixtures/prime-fleets.ts";
import { isLive, primeSession } from "./fixtures/prime-session.ts";
import { awaitTurn, processed, runWord, turnDetail, turnsOf, type FindTurn, type Turn } from "./fixtures/prime-turns.ts";
import { GATE_STATUS } from "../../src/constants/approvals.ts";

const fixture = primeFixture("watch-dog");
const access = entry(fixture.asks_for, 0, "access request");
const reach = field(access.scope, "access scope").access;
const repository = field(fixture.repository, "repository");
const subscription = wake(fixture, "app_event");
const events = field(subscription.events, "trigger events");

// A test account's fine-grained token, limited to the fixture's repository.
const TOKEN_VARIABLE = "ACCEPTANCE_GITHUB_TOKEN";
// A create, the run that asks, and the run the approval resumes.
const SETUP_TIMEOUT_MS = 600_000;
const STEP_TIMEOUT_MS = 300_000;
// GitHub's delivery, then one run on a runner.
const WAKE_TIMEOUT_MS = 300_000;
// Long enough for a second delivery of the same event to have become a second turn.
const SECOND_WAKE_GRACE_MS = 20_000;
// Branch, commit, pull request and their reads, then the close and the delete.
const GITHUB_CALLS = 6;
// The wake and the grace after it, each GitHub call at its own bound, and one step's
// worth of turn reads, so the step fails inside its own `finally` and never outside it.
const PULL_STEP_TIMEOUT_MS = WAKE_TIMEOUT_MS + SECOND_WAKE_GRACE_MS + GITHUB_CALLS * GITHUB_REQUEST_TIMEOUT_MS + STEP_TIMEOUT_MS;
const CONNECTOR_GITHUB = "github";
// `ACTOR_APP_GITHUB` in afd_api_ingress/src/handler/webhook/app_route.rs: the actor an App delivery's turn records.
const GITHUB_ACTOR = "github-app";
// GitHub's type for an account a person holds; an App acts as a `Bot`.
const PERSON_ACCOUNT = "User";
// GitHub's action for a pull request that has just opened.
const PULL_OPENED = "opened";
// The one-file change and the pull request say what they are, for anyone who finds them before the journey closes them.
const CHANGE_NOTE = `Opened by the ${fixture.name} acceptance journey, which closes it and deletes its branch.`;

const RepositorySchema = Schema.Struct({ default_branch: Schema.String });
const RefSchema = Schema.Struct({ object: Schema.Struct({ sha: Schema.String }) });
const PullSchema = Schema.Struct({
  number: Schema.Number,
  user: Schema.Struct({ login: Schema.String, type: Schema.String }),
});
// `PullRequestDigest` in afd_api_wire/src/ingress/mod.rs: a GitHub wake's stored request, as far as the journey reads it.
const DigestSchema = Schema.Struct({
  action: Schema.String,
  number: Schema.Number,
  author: Schema.String,
  head_ref: Schema.String,
});
// The scope a GitHub grant records: `RepositoryBinding::recorded` in af_fleet_runtime.
const ScopeSchema = Schema.Struct({ repositories: Schema.Array(Schema.String), access: Schema.String });
const TriggerPageSchema = Schema.Struct({
  items: Schema.Array(Schema.Struct({
    kind: Schema.String,
    source: Schema.String,
    events: Schema.NullOr(Schema.Array(Schema.String)),
    repositories: Schema.NullOr(Schema.Array(Schema.String)),
  })),
});

if (!isLive) {
  describe("prime-watch-dog-live.spec.ts", () => {
    it.skip(`requires ${ACCEPTANCE_TARGET_ENV} to be an https URL`, () => {});
  });
} else if (!onDeployedDev) {
  describe("prime-watch-dog-live.spec.ts", () => {
    it.skip(DEPLOYED_DEV_ONLY, () => {});
  });
} else {
  describe("watch-dog: a fleet reads one repository, and a pull request a person opens there wakes it", () => {
    const session = primeSession(connectedWorkspace(CONNECTOR_GITHUB));
    const repo = `/repos/${repository}`;
    let token = "";
    let base = "";
    let fleetId = "";
    let asked: Asked | null = null;
    let woke: { number: number; reply: string | null } | null = null;
    // Each GitHub-sent turn's pull request branch, read once: a turn's stored request never changes.
    const branchOf = new Map<string, string>();

    /** Reads the branch of every GitHub-sent turn not read yet. */
    const learnBranches = async (turns: ReadonlyArray<Turn>): Promise<void> => {
      for (const turn of turns) {
        if (turn.actor !== GITHUB_ACTOR || branchOf.has(turn.turn_id)) continue;
        const detail = await turnDetail(session, fleetId, turn.turn_id);
        branchOf.set(turn.turn_id, Schema.decodeUnknownSync(DigestSchema)(JSON.parse(detail.request_json)).head_ref);
      }
    };
    /** The answered turn GitHub sent for the pull request from `branch`. */
    const sentFor = (branch: string): FindTurn => async (turns) => {
      await learnBranches(turns);
      return processed((turn) => branchOf.get(turn.turn_id) === branch)(turns);
    };

    beforeAll(async () => {
      token = requireVariables([TOKEN_VARIABLE])[TOKEN_VARIABLE];
      base = Schema.decodeUnknownSync(RepositorySchema)(await callGithub(token, "GET", repo)).default_branch;
      const created = await primeFleetBeside(session, fixture,
        `The repository is ${repository}, and its pull requests open against the ${base} branch.`);
      fleetId = created.fleet_id;
      asked = await raisedCard(session, fleetId, Promise.resolve("the fleet's first message told it to ask"));
    }, SETUP_TIMEOUT_MS);

    it("the fleet's request raises one card naming GitHub, the repository and how far it reaches", async () => {
      assert.ok(asked, "the fleet raised no access request");
      assert.equal(asked.card.evidence?.service, access.name);
      assert.equal(asked.card.evidence?.kind, GRANT_KIND.integration);
      const scope = Schema.decodeUnknownSync(ScopeSchema)(asked.card.evidence?.scope);
      assert.deepEqual(scope.repositories, [repository]);
      assert.equal(scope.access, reach);

      const grant = await grantFor(session, fleetId, access.name);
      assert.equal(grant.status, GRANT_STATUS.pending, `asking is not consent: ${JSON.stringify(grant)}`);
      assert.equal(grant.scope, null, `a scope is in force before anyone approved it: ${JSON.stringify(grant)}`);
    });

    it("one approve puts that repository in force and resumes the turn that asked", async () => {
      assert.ok(asked, "the fleet raised no access request");
      const decision = await decide(session, "approve", asked.card.gate_id);
      assert.equal(decision.outcome, GATE_STATUS.approved);

      const grant = await grantFor(session, fleetId, access.name);
      assert.equal(grant.kind, GRANT_KIND.integration);
      assert.equal(grant.status, GRANT_STATUS.approved);
      const scope = Schema.decodeUnknownSync(ScopeSchema)(grant.scope);
      assert.deepEqual(scope.repositories, [repository]);
      assert.equal(scope.access, reach);
      await resumedTurn(session, fleetId, asked.waiting);
    }, STEP_TIMEOUT_MS);

    it("the GitHub trigger the fleet adds shows in `fleet triggers list`", async () => {
      assert.ok(fleetId, "the fleet was not created");
      const listed = Schema.decodeUnknownSync(TriggerPageSchema)(await session.json(["fleet", "triggers", "list", fleetId])).items;
      const mine = listed.filter((row) => row.kind === subscription.kind && row.source === subscription.source);
      assert.equal(mine.length, 1, `expected one GitHub trigger; fleet triggers list shows ${JSON.stringify(listed)}`);
      const trigger = entry(mine, 0, "trigger");
      assert.deepEqual(trigger.repositories, [repository]);
      assert.deepEqual(trigger.events, [...events]);
    });

    it("a pull request a person opens on the repository wakes the fleet once, from GitHub's own event", async () => {
      assert.ok(fleetId, "the fleet was not created");
      // The run's word names the branch and its one file, so this pull request is told apart from any other on the repository.
      const branch = runWord(`${fixture.name}-`);
      const from = Schema.decodeUnknownSync(RefSchema)(await callGithub(token, "GET", `${repo}/git/ref/heads/${base}`));
      await callGithub(token, "POST", `${repo}/git/refs`, { ref: `refs/heads/${branch}`, sha: from.object.sha });
      let number: number | null = null;
      try {
        await callGithub(token, "PUT", `${repo}/contents/${branch}.md`,
          { branch, message: `Add ${branch}.md`, content: Buffer.from(`${CHANGE_NOTE}\n`).toString("base64") });
        const opened = Schema.decodeUnknownSync(PullSchema)(await callGithub(token, "POST", `${repo}/pulls`,
          { head: branch, base, title: `Acceptance: ${branch}`, body: CHANGE_NOTE }));
        number = opened.number;
        assert.equal(opened.user.type, PERSON_ACCOUNT, `the account ${TOKEN_VARIABLE} acts as is not a person's`);

        const woken = await awaitTurn(session, fleetId, sentFor(branch),
          `answered turn GitHub sent for pull request ${number}`, WAKE_TIMEOUT_MS);
        const detail = await turnDetail(session, fleetId, woken.turn_id);
        assert.deepEqual(Schema.decodeUnknownSync(DigestSchema)(JSON.parse(detail.request_json)),
          { action: PULL_OPENED, number, author: opened.user.login, head_ref: branch });
        // One pull request opened is one wake: a second turn would be the same event taken twice.
        await Bun.sleep(SECOND_WAKE_GRACE_MS);
        const turns = await turnsOf(session, fleetId);
        await learnBranches(turns);
        assert.deepEqual(turns.filter((turn) => branchOf.get(turn.turn_id) === branch).map((turn) => turn.turn_id), [woken.turn_id]);
        woke = { number, reply: detail.response_text };
      } finally {
        // Closing is a pull request event too and wakes the fleet again, so it waits until the one wake is counted.
        try {
          if (number !== null) await callGithub(token, "PATCH", `${repo}/pulls/${number}`, { state: "closed" });
        } finally {
          await callGithub(token, "DELETE", `${repo}/git/refs/heads/${branch}`);
        }
      }
    }, PULL_STEP_TIMEOUT_MS);

    it("the fleet's reply names the pull request's number", () => {
      assert.ok(woke, "no pull request woke the fleet");
      // The number as a whole: a reply naming 120 does not name pull request 12.
      assert.match(woke.reply ?? "", new RegExp(`(?<![0-9])${woke.number}(?![0-9])`, "u"),
        `the reply to pull request ${woke.number} does not name it`);
    });
  });
}
