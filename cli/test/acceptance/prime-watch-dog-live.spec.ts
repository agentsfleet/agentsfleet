// The GitHub journey, from a terminal: watch-dog asks for one repository, a
// person approves it once, and the fleet subscribes to that repository's pull
// requests.
//
// The fleet's name, first message, repository and the access it asks for are
// `tests/fixtures/fleets/watch-dog.json`. The journey holds no GitHub
// credential and runs in the workspace that connected the platform's GitHub
// App.
//
// It stops at the trigger. The fixture's two follow-ups have the fleet open a
// pull request and then close it, and the daemon this journey runs against
// admits neither end of that: a write-granted run may open a pull request only
// from its own `agentsfleet-repair/` branch (afd_gate/src/policy/egress/write.rs),
// the App route drops every pull request event for such a branch
// (afd_api_ingress/src/handler/webhook/github/mod.rs), and no request rule
// closes a pull request or deletes a branch. Sent here, the first follow-up
// would leave a pull request on a public repository that nothing could remove.

import { describe, it, beforeAll } from "bun:test";
import assert from "node:assert/strict";
import { Schema } from "effect";

import { ACCEPTANCE_TARGET_ENV } from "./fixtures/constants.ts";
import { GRANT_KIND, GRANT_STATUS, decide, grantFor, raisedCard, resumedTurn, type Asked } from "./fixtures/grant-ops.ts";
import { DEPLOYED_DEV_ONLY, connectedWorkspace, onDeployedDev, primeFleetBeside } from "./fixtures/prime-connected.ts";
import { entry, field, primeFleet as primeFixture, wake } from "./fixtures/prime-fleets.ts";
import { isLive, primeSession } from "./fixtures/prime-session.ts";
import { GATE_STATUS } from "../../src/constants/approvals.ts";

const fixture = primeFixture("watch-dog");
const access = entry(fixture.asks_for, 0, "access request");
const reach = field(access.scope, "access scope").access;
const repository = field(fixture.repository, "repository");
const subscription = wake(fixture, "app_event");
const events = field(subscription.events, "trigger events");

// A create, the run that asks, and the run the approval resumes.
const SETUP_TIMEOUT_MS = 600_000;
const STEP_TIMEOUT_MS = 300_000;
const CONNECTOR_GITHUB = "github";
const GITHUB_API = "https://api.github.com";
const GITHUB_TIMEOUT_MS = 30_000;

const RepositorySchema = Schema.Struct({ default_branch: Schema.String });
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

/**
 * The branch the repository's pull requests open against, which a write scope
 * must name. Read as anyone reads a public repository: with no token.
 */
async function defaultBranch(): Promise<string> {
  const response = await fetch(`${GITHUB_API}/repos/${repository}`, {
    headers: { Accept: "application/vnd.github+json", "User-Agent": "acceptance-suite" },
    redirect: "error",
    signal: AbortSignal.timeout(GITHUB_TIMEOUT_MS),
  });
  assert.equal(response.status, 200, `GitHub answered ${response.status} to an unauthenticated read of ${repository}`);
  return Schema.decodeUnknownSync(RepositorySchema)(await response.json()).default_branch;
}

if (!isLive) {
  describe("prime-watch-dog-live.spec.ts", () => {
    it.skip(`requires ${ACCEPTANCE_TARGET_ENV} to be an https URL`, () => {});
  });
} else if (!onDeployedDev) {
  describe("prime-watch-dog-live.spec.ts", () => {
    it.skip(DEPLOYED_DEV_ONLY, () => {});
  });
} else {
  describe("watch-dog: a fleet is granted one repository and subscribes to its pull requests", () => {
    const session = primeSession(connectedWorkspace(CONNECTOR_GITHUB));
    let fleetId = "";
    let asked: Asked | null = null;

    beforeAll(async () => {
      const base = await defaultBranch();
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
  });
}
