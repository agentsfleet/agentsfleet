/**
 * prime-watch-dog.spec.ts — the GitHub journey. Runs against a deployed stack
 * only: GitHub delivers its events there and never to a stack a machine
 * started for itself.
 *
 * A fleet asks for one repository, opens a small pull request in it, is woken
 * by GitHub's own event for that pull request, and closes it. The fleet, the
 * repository, what a person types and the five lines this journey proves are
 * `tests/fixtures/fleets/watch-dog.json`; each test is named by the line it
 * asserts.
 *
 * Every GitHub effect is the fleet's own, made with the token the daemon mints
 * for it from the platform's GitHub App. This journey holds no GitHub
 * credential: it reads the public repository to see what the fleet did, and a
 * pull request the fleet leaves open is reported, never closed from here.
 */
import { expect, test, type Page } from "@playwright/test";
import { z } from "zod";
import type { ConnectorCatalogEntry } from "@/lib/api/connectors";
import type { TurnDetail } from "@/lib/api/turns";
import { GITHUB_GRANT, githubScope, TRIGGER_KIND, TriggerSchema } from "@/lib/api/fleet-detail";
import { signInAs } from "./fixtures/auth";
import { clientFor } from "./fixtures/api-client";
import { awaitTerminalTurn, messageFleet } from "./fixtures/chat-walk";
import { POST_DEPLOY_TAG } from "./fixtures/constants";
import { assertPassed, classifyTerminalTurn, JOURNEY_LEG, threadTurnIds, TURN_KIND_CHAT, TURN_KIND_CONTINUATION, TURN_STATUS } from "./fixtures/execution";
import { CARD_DECISION, decideFleetRequest, openFleetRequest } from "./fixtures/fleet-approval";
import { GRANT_STATUS, listGrantsForFleet, pendingGateFor, type IntegrationGrant } from "./fixtures/grants";
import { readGithub } from "./fixtures/live-services";
import { observed, POLL_INTERVAL_MS, pollFor, uniqueTag } from "./fixtures/observation";
import { createPrimeFleet, entry, PRIME_FLEET, primeFleet, proofs, wake, type PrimedFleet } from "./fixtures/prime-fleets";
import { callTo, PRIME_PERSON, settledTurn, threadOf } from "./fixtures/prime-walk";
import { getDefaultWorkspaceId } from "./fixtures/seed";
import { purgeFixtureFleet } from "./fixtures/teardown";

const fixture = primeFleet(PRIME_FLEET.watchDog);
const proof = proofs(fixture, 5);
const wanted = entry(fixture, fixture.asks_for, 0, "access");
const subscription = wake(fixture, "app_event");
const STEP_TIMEOUT_MS = 900_000;
const RENDER_TIMEOUT_MS = 15_000;
// A checkout, a push and a pull request from a sandbox, behind the runner queue.
const WORK_TIMEOUT_MS = 600_000;
// GitHub's delivery, then one run on a runner.
const WAKE_TIMEOUT_MS = 300_000;
// Long enough for a second delivery of the same event to have become a second turn.
const SECOND_WAKE_GRACE_MS = POLL_INTERVAL_MS * 10;
const REQUEST_ACCESS = "request_access";
const KIND_INTEGRATION = "integration";
/** The actor an App delivery's turn records (`ACTOR_APP_GITHUB`). */
const GITHUB_ACTOR = "github-app";
const BOT_ACCOUNT = "Bot";
const TriggersSchema = z.object({items: z.array(TriggerSchema)});
const RepositorySchema = z.object({default_branch: z.string().min(1)});
const PullSchema = z.object({number: z.number().int(), state: z.string(), head: z.object({ref: z.string()}), user: z.object({type: z.string()})});
const RefSchema = z.object({ref: z.string()});

test.describe.serial(`prime fleet ${fixture.name}`, {tag: POST_DEPLOY_TAG}, () => {
  test.setTimeout(STEP_TIMEOUT_MS);
  let workspaceId = "";
  let fleet: PrimedFleet | null = null;
  let opened: {number: number; branch: string; token: string} | null = null;
  const repository = fixture.repository ?? "";

  const primed = (): PrimedFleet => {
    if (fleet === null) throw new Error(`${fixture.name} was not created; nothing to walk`);
    return fleet;
  };
  const pull = () => {
    if (opened === null) throw new Error("the fleet opened no pull request; nothing to read");
    return opened;
  };
  const grant = async (): Promise<IntegrationGrant> => {
    const rows = (await observed(JOURNEY_LEG.observe, () => listGrantsForFleet(PRIME_PERSON, workspaceId, primed().id)))
      .filter((row) => row.name === GITHUB_GRANT);
    expect(rows, "one grant row per fleet and name").toHaveLength(1);
    return entry(fixture, rows, 0, "grant");
  };
  const wakes = async (): Promise<TurnDetail[]> => (await threadOf(workspaceId, primed().id)).filter((turn) => turn.actor === GITHUB_ACTOR);
  /** Types a follow-up and waits for its turn, however long the work in it takes. */
  const ask = async (page: Page, message: string): Promise<TurnDetail> => {
    const seen = await messageFleet(page, workspaceId, primed().id, message);
    const turn = await settledTurn(workspaceId, primed().id, (row) => row.event_type === TURN_KIND_CHAT && !seen.has(row.turn_id), WORK_TIMEOUT_MS);
    if (turn === null) throw new Error(`the fleet was still working ${WORK_TIMEOUT_MS / 1000} s after it was asked`);
    assertPassed(classifyTerminalTurn(turn));
    return turn;
  };

  test.beforeAll(async () => {
    test.setTimeout(STEP_TIMEOUT_MS);
    if (!repository) throw new Error(`${fixture.name}.json names no repository`);
    workspaceId = await getDefaultWorkspaceId(PRIME_PERSON);
    const catalogue = await clientFor(PRIME_PERSON).get<ConnectorCatalogEntry[]>(`/v1/workspaces/${workspaceId}/connectors`);
    // Connecting is a person's consent on GitHub, done once for the fixture workspace; no journey can give it.
    if (!catalogue.some((row) => row.id === GITHUB_GRANT && row.connected)) {
      throw new Error(`the fixture workspace has not connected GitHub; connect the platform's GitHub App to ${repository} in Connected apps`);
    }
    const base = await readGithub(`/repos/${repository}`, RepositorySchema);
    if (base === null) throw new Error(`${repository} is not a public repository GitHub can show`);
    fleet = await observed(JOURNEY_LEG.install, () => createPrimeFleet(PRIME_PERSON, workspaceId, fixture,
      `${fixture.first_message} The repository is ${repository}, and pull requests open against its branch ${base.default_branch}.`));
  });

  test.afterAll(async () => {
    if (fleet) await purgeFixtureFleet(PRIME_PERSON, workspaceId, fleet.id);
  });

  test(proof(0), async ({page}) => {
    await signInAs(page, PRIME_PERSON);
    const first = entry(fixture, (await threadOf(workspaceId, primed().id)).filter((turn) => turn.turn_id === primed().turnId), 0, "first turn");
    expect(first.status, "the asking run ends waiting on a person").toBe(TURN_STATUS.waiting);
    expect(callTo(first, REQUEST_ACCESS)).toMatchObject({name: wanted.name});
    const gate = await observed(JOURNEY_LEG.observe, () => pendingGateFor(PRIME_PERSON, workspaceId, primed().id));
    if (gate === null) throw new Error(`the first turn called ${REQUEST_ACCESS} and no card is pending`);
    const asked = await grant();
    expect(asked).toMatchObject({kind: KIND_INTEGRATION, status: GRANT_STATUS.pending});
    expect(githubScope(asked.requested_scope)).toEqual({repositories: [repository], access: wanted.scope?.access});
    const card = await openFleetRequest(page, workspaceId, primed().id, gate, RENDER_TIMEOUT_MS);
    await expect(page.locator("#fleet-approvals [data-approval-gate]")).toHaveCount(1);
    await expect(card.getByRole("definition").filter({hasText: repository})).toBeVisible();
    await expect(card.getByRole("definition").filter({hasText: "Read and write"})).toBeVisible();
  });

  test(proof(1), async ({page}) => {
    await signInAs(page, PRIME_PERSON);
    const gate = await pendingGateFor(PRIME_PERSON, workspaceId, primed().id);
    if (gate === null) throw new Error("no card is pending; nothing to approve");
    const before = await threadTurnIds(PRIME_PERSON, workspaceId, primed().id);
    await decideFleetRequest(page, workspaceId, primed().id, gate, CARD_DECISION.approve, RENDER_TIMEOUT_MS);
    const resumed = await awaitTerminalTurn(workspaceId, primed().id, before, TURN_KIND_CONTINUATION);
    assertPassed(classifyTerminalTurn(resumed));
    expect(resumed.resumes_turn_id, "the answer resumes the turn that asked").toBe(primed().turnId);
    const approved = await grant();
    expect(approved.status).toBe(GRANT_STATUS.approved);
    expect(githubScope(approved.scope)).toEqual({repositories: [repository], access: wanted.scope?.access});
    // The resumed turn goes on to what the first message asked next: the fleet's own GitHub trigger.
    const triggers = TriggersSchema.parse(await clientFor(PRIME_PERSON).get<unknown>(`/v1/workspaces/${workspaceId}/fleets/${primed().id}/triggers`)).items;
    expect(triggers.map((row) => [row.kind, row.source, row.repositories, row.events]))
      .toEqual([[TRIGGER_KIND.appEvent, subscription.source, [repository], subscription.events]]);
  });

  test(proof(2), async ({page}) => {
    await signInAs(page, PRIME_PERSON);
    const token = uniqueTag();
    const branch = `${fixture.name}-${token}`;
    await ask(page, `${entry(fixture, fixture.follow_ups, 0, "follow-up")} The token is ${token}. Name the pull request's branch ${branch}.`);
    const found = await pollFor(async () => {
      const pulls = await readGithub(`/repos/${repository}/pulls?state=all&sort=created&direction=desc&per_page=30`, z.array(PullSchema));
      return pulls?.find((row) => row.head.ref === branch) ?? null;
    }, WAKE_TIMEOUT_MS);
    if (found === null) throw new Error(`${repository} holds no pull request from the branch ${branch}`);
    expect(found.state).toBe("open");
    // GitHub records who opened it: the App's own account, which only a token minted from the App acts as.
    expect(found.user.type).toBe(BOT_ACCOUNT);
    opened = {number: found.number, branch, token};
  });

  test(proof(3), async () => {
    const woken = await settledTurn(workspaceId, primed().id, (turn) => turn.actor === GITHUB_ACTOR, WAKE_TIMEOUT_MS);
    if (woken === null) throw new Error(`GitHub's event for pull request ${pull().number} woke no turn in ${WAKE_TIMEOUT_MS / 1000} s`);
    assertPassed(classifyTerminalTurn(woken));
    expect(woken.response_text).toContain(String(pull().number));
    // One pull request opened is one wake: a second turn would be the same event taken twice.
    await new Promise((resolve) => setTimeout(resolve, SECOND_WAKE_GRACE_MS));
    expect((await wakes()).map((turn) => turn.turn_id)).toEqual([woken.turn_id]);
  });

  test(proof(4), async ({page}) => {
    await signInAs(page, PRIME_PERSON);
    await ask(page, `${entry(fixture, fixture.follow_ups, 1, "follow-up")} It is pull request ${pull().number}, on the branch ${pull().branch}.`);
    const closed = await readGithub(`/repos/${repository}/pulls/${pull().number}`, PullSchema);
    expect(closed?.state, `the fleet left pull request ${pull().number} open in ${repository}; close it by hand`).toBe("closed");
    const left = await readGithub(`/repos/${repository}/git/ref/heads/${pull().branch}`, RefSchema);
    expect(left, `the fleet left the branch ${pull().branch} in ${repository}; delete it by hand`).toBeNull();
  });
});
