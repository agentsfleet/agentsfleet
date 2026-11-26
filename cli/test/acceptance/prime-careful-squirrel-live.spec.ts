// The access journey, from a terminal: careful-squirrel asks for a secret, a
// person answers once, and the run that asked resumes.
//
// The fleet's name, its messages and the secret it asks for are
// `tests/fixtures/fleets/careful-squirrel.json`. Two fleets walk it, one after
// the other under the fixture's name: the first is approved and then revoked,
// the second is denied. Every assertion reads what the request and the answer
// wrote (the card, the grant, the turns), never what the model said about them.

import { describe, it, beforeAll } from "bun:test";
import assert from "node:assert/strict";
import crypto from "node:crypto";

import { ACCEPTANCE_TARGET_ENV, AGENTSFLEET_STATUS } from "./fixtures/constants.ts";
import {
  GRANT_KIND,
  GRANT_STATUS,
  accessCards,
  decide,
  grantFor,
  raisedCard,
  resumedTurn,
  showCard,
  type Asked,
} from "./fixtures/grant-ops.ts";
import { getStatus } from "./fixtures/lifecycle.ts";
import { refusalCode } from "./fixtures/negatives.ts";
import { entry, primeFleet as primeFixture, said } from "./fixtures/prime-fleets.ts";
import { isLive, ownWorkspace, primeFleet, primeSession, storeSecret } from "./fixtures/prime-session.ts";
import { randomUuidv7 } from "./helpers-lifecycle-with-token.ts";
import { GATE_STATUS } from "../../src/constants/approvals.ts";
import { EXIT_CODE } from "../../src/errors/index.ts";

const fixture = primeFixture("careful-squirrel");
const followUp = entry(fixture.follow_ups, 0, "follow-up");
const secret = entry(fixture.asks_for, 0, "access request");
const WORKSPACE = `fixture-cli-${fixture.journey}`;

// A create, the asking run and the run the answer resumes.
const JOURNEY_TIMEOUT_MS = 720_000;
const SECRET_BYTES = 24;
// af_core/src/error_code/request.rs: a second, different answer, and a card nobody holds.
const CODE_ALREADY_RESOLVED = "UZ-APPROVAL-006";
const CODE_GATE_NOT_FOUND = "UZ-APPROVAL-002";
const APPROVALS = ["fleet", "approvals"] as const;

if (!isLive) {
  describe("prime-careful-squirrel-live.spec.ts", () => {
    it.skip(`requires ${ACCEPTANCE_TARGET_ENV} to be an https URL`, () => {});
  });
} else {
  describe("careful-squirrel: a fleet holds a secret only after a person approves it", () => {
    const session = primeSession(ownWorkspace(WORKSPACE));

    /** A new careful-squirrel that has asked for the fixture's secret and is waiting on its card. */
    async function squirrelThatAsked(): Promise<{ readonly fleetId: string; readonly asked: Asked }> {
      const prompt = said(followUp, `The secret is named ${secret.name}. Call request_access and no other tool. ` +
        "Once the person answers, call no tool and reply in one line saying whether you hold the secret. Never print its value.");
      const created = await primeFleet(session, fixture, prompt);
      const asked = await raisedCard(session, created.fleet_id, Promise.resolve("the first message asked for the fixture secret"));
      assert.equal(asked.waiting.turn_id, created.turn_id);
      return { fleetId: created.fleet_id, asked };
    }

    beforeAll(async () => {
      // A real vault entry with a value of this run's. Nothing upstream reads
      // it: the journey proves who may hold the secret, not a call made with it.
      await storeSecret(session, secret.name, { token: crypto.randomBytes(SECRET_BYTES).toString("hex") });
    });

    describe("approved, then revoked", () => {
      let fleetId = "";
      let asked: Asked | null = null;

      beforeAll(async () => {
        ({ fleetId, asked } = await squirrelThatAsked());
      }, JOURNEY_TIMEOUT_MS);

      it("the fleet's request raises one pending card and one pending grant, named for what it asked", async () => {
        assert.ok(asked, "the fleet raised no access request");
        const shown = await showCard(session, asked.card.gate_id);
        assert.equal(shown.fleet_id, fleetId);
        assert.equal(shown.status, GATE_STATUS.pending);
        assert.equal(shown.resolved_at, null);
        assert.deepEqual(shown.evidence, { service: secret.name, kind: GRANT_KIND.secret, scope: null });
        assert.ok(shown.proposed_action.includes(secret.name), `the card does not name the secret: ${shown.proposed_action}`);
        assert.ok(shown.blast_radius.includes(secret.name), `the card does not say how far the access reaches: ${shown.blast_radius}`);

        const grant = await grantFor(session, fleetId, secret.name);
        assert.equal(grant.kind, GRANT_KIND.secret);
        assert.equal(grant.status, GRANT_STATUS.pending, `asking is not consent: ${JSON.stringify(grant)}`);
        assert.equal(grant.approved_at, null, `a pending grant cannot carry approved_at: ${JSON.stringify(grant)}`);
      });

      it("one approve grants the access and resumes the run that asked", async () => {
        assert.ok(asked, "the fleet raised no access request");
        const decision = await decide(session, "approve", asked.card.gate_id);
        assert.equal(decision.gate_id, asked.card.gate_id);
        assert.equal(decision.outcome, GATE_STATUS.approved);
        assert.ok(decision.resolved_by.length > 0, "a decision must name who made it");

        const grant = await grantFor(session, fleetId, secret.name);
        assert.equal(grant.status, GRANT_STATUS.approved);
        assert.ok(grant.approved_at, `an approved grant must carry approved_at: ${JSON.stringify(grant)}`);
        const cards = await accessCards(session, fleetId);
        assert.deepEqual(cards.map((card) => card.status), [GATE_STATUS.approved], `the inbox still holds a request: ${JSON.stringify(cards)}`);

        await resumedTurn(session, fleetId, asked.waiting);
        assert.equal((await getStatus(session.env(), fleetId)).status, AGENTSFLEET_STATUS.active);
      }, JOURNEY_TIMEOUT_MS);

      it("`fleet grants revoke` takes the access away, moving the grant and adding none", async () => {
        assert.ok(asked, "the fleet raised no access request");
        const approved = await grantFor(session, fleetId, secret.name);
        const revoked = await session.run(["fleet", "grants", "revoke", approved.id, "--fleet", fleetId]);
        assert.equal(revoked.code, 0, `fleet grants revoke exited ${revoked.code}: ${revoked.stderr}`);

        const grant = await grantFor(session, fleetId, secret.name);
        assert.equal(grant.id, approved.id);
        assert.equal(grant.status, GRANT_STATUS.revoked, `the grant still stands: ${JSON.stringify(grant)}`);
      });

      it("`approvals show` on a card this workspace does not hold is refused as unknown", async () => {
        const refused = await session.run([...APPROVALS, "show", randomUuidv7(), "--json"]);
        assert.equal(refused.code, EXIT_CODE.ServerError, `an unknown card answered: ${refused.stdout}`);
        assert.equal(refusalCode(refused), CODE_GATE_NOT_FOUND);
      });
    });

    describe("denied", () => {
      let fleetId = "";
      let asked: Asked | null = null;

      beforeAll(async () => {
        ({ fleetId, asked } = await squirrelThatAsked());
      }, JOURNEY_TIMEOUT_MS);

      it("`approvals deny` answers denied, revokes the grant, and resumes the run without the access", async () => {
        assert.ok(asked, "the second fleet raised no access request");
        const decision = await decide(session, "deny", asked.card.gate_id);
        assert.equal(decision.outcome, GATE_STATUS.denied);
        assert.ok(decision.resolved_by.length > 0, "a decision must name who made it");

        const shown = await showCard(session, asked.card.gate_id);
        assert.equal(shown.status, GATE_STATUS.denied);
        assert.equal(shown.resolved_at, decision.resolved_at);
        const cards = await accessCards(session, fleetId);
        assert.deepEqual(cards.map((card) => card.status), [GATE_STATUS.denied], `the fleet still has a request waiting: ${JSON.stringify(cards)}`);

        const grant = await grantFor(session, fleetId, secret.name);
        assert.equal(grant.status, GRANT_STATUS.revoked, `a denied request left the grant: ${JSON.stringify(grant)}`);
        assert.equal(grant.approved_at, null, "a denied grant was never approved");

        await resumedTurn(session, fleetId, asked.waiting);
        assert.equal((await getStatus(session.env(), fleetId)).status, AGENTSFLEET_STATUS.active,
          "a denial answers the run; it does not stop the fleet");
      }, JOURNEY_TIMEOUT_MS);

      it("approving a denied request is refused with the decision that stands", async () => {
        assert.ok(asked, "the second fleet raised no access request");
        const late = await session.run([...APPROVALS, "approve", asked.card.gate_id, "--json"]);
        assert.equal(late.code, EXIT_CODE.ServerError, `a decided request took a second answer: ${late.stdout}`);
        assert.equal(refusalCode(late), CODE_ALREADY_RESOLVED);
        assert.equal((await showCard(session, asked.card.gate_id)).status, GATE_STATUS.denied, "the late approval overturned the denial");
        assert.equal((await grantFor(session, fleetId, secret.name)).status, GRANT_STATUS.revoked);
      });

      it("`approvals deny` on a card this workspace does not hold is refused as unknown", async () => {
        const refused = await session.run([...APPROVALS, "deny", randomUuidv7(), "--json"]);
        assert.equal(refused.code, EXIT_CODE.ServerError, `an unknown card was decided: ${refused.stdout}`);
        assert.equal(refusalCode(refused), CODE_GATE_NOT_FOUND);
      });
    });
  });
}
