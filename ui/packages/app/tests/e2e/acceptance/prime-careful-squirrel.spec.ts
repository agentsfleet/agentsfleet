/**
 * prime-careful-squirrel.spec.ts — the access journey.
 *
 * A fleet holds nothing until a person approves what it asked for. The fleet,
 * the secret it asks for, the message a person sends and the four lines this
 * journey proves are `tests/fixtures/fleets/careful-squirrel.json`; each test
 * is named by the line it asserts.
 *
 * One fleet walks the whole chain: it asks and a card is raised, Approve
 * resumes the asking turn, a revoke makes its next run ask again, and Deny
 * resumes that turn without the secret. The API does not expose the secrets a
 * run was handed, so what a run held is read from two facts the API does show:
 * the grant row was revoked before the run was sent, and the daemon's answer
 * to the run's own `request_access`, saved in its tool trace, is the one it
 * gives a run that does not hold the secret.
 */
import { expect, test, type Page } from "@playwright/test";
import { z } from "zod";
import type { ApprovalGate } from "@/lib/api/approvals";
import type { TurnDetail } from "@/lib/api/turns";
import { signInAs } from "./fixtures/auth";
import { clientFor } from "./fixtures/api-client";
import { awaitTerminalTurn } from "./fixtures/chat-walk";
import { POST_DEPLOY_TAG } from "./fixtures/constants";
import { assertPassed, classifyTerminalTurn, JOURNEY_LEG, threadTurnIds, TURN_KIND_CONTINUATION, TURN_STATUS } from "./fixtures/execution";
import { CARD_DECISION, decideFleetRequest, openFleetRequest, type CardDecision } from "./fixtures/fleet-approval";
import { GRANT_STATUS, listGrantsForFleet, pendingGateFor, serviceOn, type IntegrationGrant } from "./fixtures/grants";
import { gotoWorkspace } from "./fixtures/nav";
import { observed, uniqueTag } from "./fixtures/observation";
import { createPrimeFleet, entry, PRIME_FLEET, primeFleet, proofs, type PrimedFleet } from "./fixtures/prime-fleets";
import { callTo, PRIME_PERSON, say } from "./fixtures/prime-walk";
import { getDefaultWorkspaceId } from "./fixtures/seed";
import { purgeFixtureFleet } from "./fixtures/teardown";

const fixture = primeFleet(PRIME_FLEET.carefulSquirrel);
const proof = proofs(fixture, 4);
const wanted = entry(fixture, fixture.asks_for, 0, "access");
const STEP_TIMEOUT_MS = 420_000;
const RENDER_TIMEOUT_MS = 15_000;
const REQUEST_ACCESS = "request_access";
const KIND_SECRET = "secret";
// The words the fleet ends its reply with once the person has answered: one per answer.
const HELD_WORD = "HELD";
const REFUSED_WORD = "REFUSED";
// The secret only has to exist and name a host the egress allowlist reads; no run sends it anywhere.
const SECRET_HOST = "example.com";
const ACCESS_COLUMN = 3;
// What `request_access` tells a run, as `afr_tools/src/verbs/access/mod.rs` words it: the run asked
// a person and does not hold the secret, or the approved grant already covers it and the run holds it.
const ASKED_ANSWER = (name: string) => `Waiting for a person to approve access to ${name}.`;
const HOLDS_ANSWER = "this run holds it";
const SecretsSchema = z.object({secrets: z.array(z.object({name: z.string(), available_to: z.array(z.object({id: z.string()})).optional()}))});

/** The fixture's follow-up with what this run adds: the secret's name, no tool but the request, and a token per answer. */
function askFor(token: string): string {
  return `${entry(fixture, fixture.follow_ups, 0, "follow-up")} The secret is named ${wanted.name}. Ask for it with ${REQUEST_ACCESS} and call no other tool. ` +
    "A secret a person approved is one you hold, which is all that reading it means here: send it nowhere. " +
    `Once the person has answered, call no tool. If they approved, end your reply with the word ${HELD_WORD}, a space, then the token ${token}. ` +
    `If they denied, end your reply with the word ${REFUSED_WORD}, a space, then the token ${token}.`;
}

type Asked = {readonly gate: ApprovalGate; readonly turn: TurnDetail; readonly token: string};

/** What the daemon answered `turn`'s one call to `tool`, as the turn's saved trace carries it. */
function answerTo(turn: TurnDetail, tool: string): string {
  const calls = turn.tool_calls?.calls.filter((made) => made.name === tool) ?? [];
  expect(calls, `turn ${turn.turn_id} calls ${tool} once`).toHaveLength(1);
  return entry(fixture, calls, 0, tool).output_head ?? "";
}

test.describe.serial(`prime fleet ${fixture.name}`, {tag: POST_DEPLOY_TAG}, () => {
  test.setTimeout(STEP_TIMEOUT_MS);
  let workspaceId = "";
  let fleet: PrimedFleet | null = null;
  let asked: Asked | null = null;

  const primed = (): PrimedFleet => {
    if (fleet === null) throw new Error(`${fixture.name} was not created; nothing to walk`);
    return fleet;
  };
  const open = (): Asked => {
    if (asked === null) throw new Error("the fleet raised no card; nothing to answer");
    return asked;
  };
  const grant = async (): Promise<IntegrationGrant> => {
    const rows = (await observed(JOURNEY_LEG.observe, () => listGrantsForFleet(PRIME_PERSON, workspaceId, primed().id)))
      .filter((row) => row.name === wanted.name);
    expect(rows, "one grant row per fleet and name").toHaveLength(1);
    return entry(fixture, rows, 0, "grant");
  };
  const holders = async (): Promise<string[]> => {
    const stored = SecretsSchema.parse(await clientFor(PRIME_PERSON).get<unknown>(`/v1/workspaces/${workspaceId}/secrets`));
    return (stored.secrets.find((secret) => secret.name === wanted.name)?.available_to ?? []).map((holder) => holder.id);
  };

  /** Sends the follow-up and reads the one card the run's request raised. */
  async function ask(page: Page): Promise<Asked> {
    const token = uniqueTag();
    const turn = await say(page, workspaceId, primed().id, askFor(token));
    expect(turn.status, "the asking run ends waiting on a person").toBe(TURN_STATUS.waiting);
    expect(callTo(turn, REQUEST_ACCESS)).toMatchObject({name: wanted.name});
    // `pendingGateFor` refuses a second pending card for one fleet, so one answer is one card.
    const gate = await observed(JOURNEY_LEG.observe, () => pendingGateFor(PRIME_PERSON, workspaceId, primed().id));
    if (gate === null) throw new Error(`turn ${turn.turn_id} called ${REQUEST_ACCESS} and no card is pending`);
    expect(serviceOn(gate)).toBe(wanted.name);
    expect(await grant()).toMatchObject({kind: KIND_SECRET, status: GRANT_STATUS.pending});
    return {gate, turn, token};
  }

  /** One click on the open card, and the turn that click resumed. */
  async function answer(page: Page, decision: CardDecision): Promise<TurnDetail> {
    const {gate, turn} = open();
    const before = await observed(JOURNEY_LEG.run, () => threadTurnIds(PRIME_PERSON, workspaceId, primed().id));
    await decideFleetRequest(page, workspaceId, primed().id, gate, decision, RENDER_TIMEOUT_MS);
    expect(await pendingGateFor(PRIME_PERSON, workspaceId, primed().id)).toBeNull();
    const resumed = await awaitTerminalTurn(workspaceId, primed().id, before, TURN_KIND_CONTINUATION);
    assertPassed(classifyTerminalTurn(resumed));
    expect(resumed.resumes_turn_id, "the answer resumes the turn that asked").toBe(turn.turn_id);
    return resumed;
  }

  test.beforeAll(async () => {
    test.setTimeout(STEP_TIMEOUT_MS);
    workspaceId = await getDefaultWorkspaceId(PRIME_PERSON);
    await clientFor(PRIME_PERSON).post(`/v1/workspaces/${workspaceId}/secrets`,
      {name: wanted.name, data: {token: `fixture-${uniqueTag()}`, host: SECRET_HOST}});
    fleet = await observed(JOURNEY_LEG.install, () => createPrimeFleet(PRIME_PERSON, workspaceId, fixture));
  });

  test.afterAll(async () => {
    if (fleet) await purgeFixtureFleet(PRIME_PERSON, workspaceId, fleet.id);
    await clientFor(PRIME_PERSON).delete(`/v1/workspaces/${workspaceId}/secrets/${encodeURIComponent(wanted.name)}`).catch(() => undefined);
  });

  test(proof(0), async ({page}) => {
    await signInAs(page, PRIME_PERSON);
    expect(await holders(), "nothing holds the secret before a person approves it").toEqual([]);
    asked = await ask(page);
    const card = await openFleetRequest(page, workspaceId, primed().id, asked.gate, RENDER_TIMEOUT_MS);
    await expect(page.locator("#fleet-approvals [data-approval-gate]")).toHaveCount(1);
    await expect(card.getByRole("definition").filter({hasText: wanted.name})).toBeVisible();
    await expect(card.getByRole("button", {name: CARD_DECISION.approve.button, exact: true})).toBeVisible();
    await expect(card.getByRole("button", {name: CARD_DECISION.deny.button, exact: true})).toBeVisible();
  });

  test(proof(1), async ({page}) => {
    await signInAs(page, PRIME_PERSON);
    const resumed = await answer(page, CARD_DECISION.approve);
    expect(resumed.response_text).toContain(`${HELD_WORD} ${open().token}`);
    const approved = await grant();
    expect(approved.status).toBe(GRANT_STATUS.approved);
    expect(approved.approved_at ?? null).not.toBeNull();
    // The secret now lists this fleet among the fleets a run resolves it for, and the Secrets page says so.
    expect(await holders()).toEqual([primed().id]);
    await gotoWorkspace(page, PRIME_PERSON, "secrets");
    const row = page.getByRole("table").getByRole("row").filter({hasText: wanted.name});
    await expect(row.getByRole("cell").nth(ACCESS_COLUMN)).toHaveText(primed().name, {timeout: RENDER_TIMEOUT_MS});
    await page.getByRole("button", {name: `Manage ${wanted.name}`, exact: true}).click();
    await expect(page.getByRole("dialog")).toContainText(`Available to ${primed().name}.`);
  });

  test(proof(3), async ({page}) => {
    await signInAs(page, PRIME_PERSON);
    const approved = await grant();
    expect(approved.status, "the walk revokes an approved grant").toBe(GRANT_STATUS.approved);
    await clientFor(PRIME_PERSON).delete(`/v1/workspaces/${workspaceId}/fleets/${primed().id}/integration-grants/${approved.id}`);
    // Read before the next run is sent, so this is the grant the run starts under.
    expect((await grant()).status).toBe(GRANT_STATUS.revoked);
    expect(await holders()).toEqual([]);
    const earlier = open().gate.gate_id;
    asked = await ask(page);
    expect(asked.gate.gate_id).not.toBe(earlier);
    // The daemon told this run it does not hold the secret; a run the approved grant covers is told it holds it.
    const answered = answerTo(asked.turn, REQUEST_ACCESS);
    expect(answered).toBe(ASKED_ANSWER(wanted.name));
    expect(answered).not.toContain(HOLDS_ANSWER);
  });

  test(proof(2), async ({page}) => {
    await signInAs(page, PRIME_PERSON);
    const resumed = await answer(page, CARD_DECISION.deny);
    expect(resumed.response_text).toContain(`${REFUSED_WORD} ${open().token}`);
    expect(resumed.response_text).not.toContain(`${HELD_WORD} ${open().token}`);
    // A deny is stored as a revoked grant, and the secret lists no fleet.
    expect((await grant()).status).toBe(GRANT_STATUS.revoked);
    expect(await holders()).toEqual([]);
  });
});
