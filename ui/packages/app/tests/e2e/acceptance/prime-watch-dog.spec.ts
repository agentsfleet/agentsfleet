/**
 * prime-watch-dog.spec.ts — the GitHub journey. Runs against a deployed stack
 * only: GitHub delivers its events there and never to a stack a machine
 * started for itself.
 *
 * A fleet asks to read one repository and subscribes to its pull requests; a
 * person opens a pull request there, and GitHub's own event for it wakes the
 * fleet, which names it. The fleet, the repository and the four lines this
 * journey proves are `tests/fixtures/fleets/watch-dog.json`; each test is
 * named by the line it asserts.
 *
 * The daemon drops GitHub events on a fleet's own `agentsfleet-repair/`
 * branches, so the event has to come from outside: the test opens the pull
 * request as a test account, with that account's fine-grained token for the
 * repository in `ACCEPTANCE_GITHUB_TOKEN`, and closes it and deletes its
 * branch with the same token once the wake is counted. The fleet only reads.
 * It runs in the fixture person's workspace that connected GitHub, and deletes
 * only its fleet.
 */
import { expect, test } from "@playwright/test";
import { z } from "zod";
import type { TurnDetail } from "@/lib/api/turns";
import { GITHUB_GRANT, githubScope, TRIGGER_KIND, TriggerSchema } from "@/lib/api/fleet-detail";
import { signInAs } from "./fixtures/auth";
import { clientFor } from "./fixtures/api-client";
import { connectedWorkspaceId } from "./fixtures/connected-workspace";
import { awaitTerminalTurn } from "./fixtures/chat-walk";
import { POST_DEPLOY_TAG } from "./fixtures/constants";
import { assertPassed, classifyTerminalTurn, JOURNEY_LEG, threadTurnIds, TURN_KIND_CONTINUATION, TURN_STATUS } from "./fixtures/execution";
import { CARD_DECISION, decideFleetRequest, openFleetRequest } from "./fixtures/fleet-approval";
import { GRANT_STATUS, listGrantsForFleet, pendingGateFor, type IntegrationGrant } from "./fixtures/grants";
import { callGithub, requiredVariable } from "./fixtures/live-services";
import { observed, POLL_INTERVAL_MS, uniqueTag } from "./fixtures/observation";
import { createPrimeFleet, entry, PRIME_FLEET, primeFleet, proofs, wake, type PrimedFleet } from "./fixtures/prime-fleets";
import { callTo, PRIME_PERSON, settledTurn, threadOf } from "./fixtures/prime-walk";
import { purgeFixtureFleet } from "./fixtures/teardown";

const fixture = primeFleet(PRIME_FLEET.watchDog);
const proof = proofs(fixture, 4);
const wanted = entry(fixture, fixture.asks_for, 0, "access");
if (wanted.scope === undefined) throw new Error(`${fixture.name}.json asks for GitHub with no reach`);
const reach = wanted.scope.access;
const subscription = wake(fixture, "app_event");
const TOKEN_VARIABLE = "ACCEPTANCE_GITHUB_TOKEN";
const STEP_TIMEOUT_MS = 900_000;
const RENDER_TIMEOUT_MS = 15_000;
// GitHub's delivery, then one run on a runner.
const WAKE_TIMEOUT_MS = 300_000;
// Long enough for a second delivery of the same event to have become a second turn.
const SECOND_WAKE_GRACE_MS = POLL_INTERVAL_MS * 10;
const REQUEST_ACCESS = "request_access";
const KIND_INTEGRATION = "integration";
/** The actor an App delivery's turn records (`ACTOR_APP_GITHUB`). */
const GITHUB_ACTOR = "github-app";
/** GitHub's type for an account a person holds; an App acts as a `Bot`. */
const PERSON_ACCOUNT = "User";
/** GitHub's action for a pull request that has just opened. */
const PULL_OPENED = "opened";
/** What the access card shows for each reach a GitHub grant can ask (`AccessRequest`). */
const REACH_SHOWN = {read: "Read only", write: "Read and write"} as const;
// The one-file change and the pull request say what they are, for anyone who finds them before the journey closes them.
const CHANGE_NOTE = `Opened by the ${fixture.name} acceptance journey, which closes it and deletes its branch.`;
const TriggersSchema = z.object({items: z.array(TriggerSchema)});
const RepositorySchema = z.object({default_branch: z.string().min(1)});
const RefSchema = z.object({object: z.object({sha: z.string().min(1)})});
const CommitSchema = z.object({commit: z.object({sha: z.string().min(1)})});
const PullSchema = z.object({number: z.number().int(), user: z.object({login: z.string(), type: z.string()})});
// `PullRequestDigest` in afd_api_wire/src/ingress/mod.rs: a GitHub wake's stored request, as far as the journey reads it.
const DigestSchema = z.object({action: z.string(), number: z.number().int(), author: z.string(), head_ref: z.string()});

test.describe.serial(`prime fleet ${fixture.name}`, {tag: POST_DEPLOY_TAG}, () => {
  test.setTimeout(STEP_TIMEOUT_MS);
  let workspaceId = "";
  let token = "";
  let base = "";
  let fleet: PrimedFleet | null = null;
  let woke: {number: number; turn: TurnDetail} | null = null;
  const repository = fixture.repository ?? "";
  const repo = `/repos/${repository}`;

  const primed = (): PrimedFleet => {
    if (fleet === null) throw new Error(`${fixture.name} was not created; nothing to walk`);
    return fleet;
  };
  const wakeUp = () => {
    if (woke === null) throw new Error("no pull request woke the fleet; nothing to read");
    return woke;
  };
  const grant = async (): Promise<IntegrationGrant> => {
    const rows = (await observed(JOURNEY_LEG.observe, () => listGrantsForFleet(PRIME_PERSON, workspaceId, primed().id)))
      .filter((row) => row.name === GITHUB_GRANT);
    expect(rows, "one grant row per fleet and name").toHaveLength(1);
    return entry(fixture, rows, 0, "grant");
  };
  /** Whether GitHub sent `turn` for the pull request from `branch`, as its stored request says. */
  const sentFor = (branch: string) => (turn: TurnDetail): boolean =>
    turn.actor === GITHUB_ACTOR && DigestSchema.parse(JSON.parse(turn.request_json)).head_ref === branch;

  test.beforeAll(async () => {
    test.setTimeout(STEP_TIMEOUT_MS);
    if (!repository) throw new Error(`${fixture.name}.json names no repository`);
    token = requiredVariable(TOKEN_VARIABLE);
    // Connecting is a person's consent on GitHub, done once in a workspace the fixture person belongs to.
    workspaceId = await connectedWorkspaceId(PRIME_PERSON, {id: GITHUB_GRANT, provider: "GitHub"});
    base = (await callGithub(token, "GET", repo, RepositorySchema)).default_branch;
    fleet = await observed(JOURNEY_LEG.install, () => createPrimeFleet(PRIME_PERSON, workspaceId, fixture,
      `${fixture.first_message} The repository is ${repository}, and pull requests open against its branch ${base}.`));
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
    expect(githubScope(asked.requested_scope)).toEqual({repositories: [repository], access: reach});
    const card = await openFleetRequest(page, workspaceId, primed().id, gate, RENDER_TIMEOUT_MS);
    await expect(page.locator("#fleet-approvals [data-approval-gate]")).toHaveCount(1);
    await expect(card.getByRole("definition").filter({hasText: repository})).toBeVisible();
    await expect(card.getByRole("definition").filter({hasText: REACH_SHOWN[reach]})).toBeVisible();
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
    expect(githubScope(approved.scope)).toEqual({repositories: [repository], access: reach});
    // The resumed turn goes on to what the first message asked next: the fleet's own GitHub trigger.
    const triggers = TriggersSchema.parse(await clientFor(PRIME_PERSON).get<unknown>(`/v1/workspaces/${workspaceId}/fleets/${primed().id}/triggers`)).items;
    expect(triggers.map((row) => [row.kind, row.source, row.repositories, row.events]))
      .toEqual([[TRIGGER_KIND.appEvent, subscription.source, [repository], subscription.events]]);
  });

  test(proof(2), async () => {
    // The run's tag names the branch and its one file, so this pull request is told apart from any other on the repository.
    const branch = `${fixture.name}-${uniqueTag()}`;
    const from = await callGithub(token, "GET", `${repo}/git/ref/heads/${base}`, RefSchema);
    await callGithub(token, "POST", `${repo}/git/refs`, RefSchema, {ref: `refs/heads/${branch}`, sha: from.object.sha});
    let number: number | null = null;
    try {
      await callGithub(token, "PUT", `${repo}/contents/${branch}.md`, CommitSchema,
        {branch, message: `Add ${branch}.md`, content: Buffer.from(`${CHANGE_NOTE}\n`).toString("base64")});
      const opened = await callGithub(token, "POST", `${repo}/pulls`, PullSchema, {head: branch, base, title: `Acceptance: ${branch}`, body: CHANGE_NOTE});
      number = opened.number;
      expect(opened.user.type, `the account ${TOKEN_VARIABLE} acts as is not a person's`).toBe(PERSON_ACCOUNT);
      const woken = await settledTurn(workspaceId, primed().id, sentFor(branch), WAKE_TIMEOUT_MS);
      if (woken === null) throw new Error(`GitHub's event for pull request ${number} woke no turn in ${WAKE_TIMEOUT_MS / 1000} s`);
      assertPassed(classifyTerminalTurn(woken));
      expect(DigestSchema.parse(JSON.parse(woken.request_json)))
        .toEqual({action: PULL_OPENED, number, author: opened.user.login, head_ref: branch});
      // One pull request opened is one wake: a second turn would be the same event taken twice.
      await new Promise((resolve) => setTimeout(resolve, SECOND_WAKE_GRACE_MS));
      expect((await threadOf(workspaceId, primed().id)).filter(sentFor(branch)).map((turn) => turn.turn_id)).toEqual([woken.turn_id]);
      woke = {number, turn: woken};
    } finally {
      // Closing is a pull request event too and wakes the fleet again, so it waits until the one wake is counted.
      try {
        if (number !== null) await callGithub(token, "PATCH", `${repo}/pulls/${number}`, PullSchema, {state: "closed"});
      } finally {
        await callGithub(token, "DELETE", `${repo}/git/refs/heads/${branch}`, z.null());
      }
    }
  });

  test(proof(3), () => {
    const {number, turn} = wakeUp();
    // The number as a whole: a reply naming 120 does not name pull request 12.
    expect(turn.response_text).toMatch(new RegExp(`(?<![0-9])${number}(?![0-9])`, "u"));
  });
});
