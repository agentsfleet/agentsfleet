/**
 * prime-stubborn-mule.spec.ts — the task journey.
 *
 * A fleet that keeps going until the job is done: its own model starts a task
 * from a person's message, writes the statements the task is judged against,
 * and the task's runs go on until the completion check answers done. The
 * fleet, what a person types and the four lines this journey proves are
 * `tests/fixtures/fleets/stubborn-mule.json`; each test is named by the line
 * it asserts.
 *
 * A completion check is read where the API records one: the task's own count
 * of decision calls, and the goal checks a turn's run recorded. The same two
 * reads find the task's checks before they are used to find none on a plain
 * message.
 */
import { expect, test } from "@playwright/test";
import { z } from "zod";
import type { TurnDetail } from "@/lib/api/turns";
import { DURABLE_TASK_STATUS } from "@/lib/api/tasks";
import { readDurableTaskActivityHistory } from "@/lib/task-activity";
import { fixtureSubject, signInAs } from "./fixtures/auth";
import { clientFor } from "./fixtures/api-client";
import { POST_DEPLOY_TAG } from "./fixtures/constants";
import { assertPassed, classifyTerminalTurn, JOURNEY_LEG } from "./fixtures/execution";
import { observed, pollFor, uniqueTag } from "./fixtures/observation";
import { createPrimeFleet, entry, PRIME_FLEET, primeFleet, proofs, type PrimedFleet } from "./fixtures/prime-fleets";
import { callTo, messageOf, PRIME_PERSON, say, threadOf, toolsCalled } from "./fixtures/prime-walk";
import { probeMessage, probeReply } from "./fixtures/probe";
import { getDefaultWorkspaceId } from "./fixtures/seed";
import { purgeFixtureFleet } from "./fixtures/teardown";

const fixture = primeFleet(PRIME_FLEET.stubbornMule);
const proof = proofs(fixture, 4);
const objective = entry(fixture, fixture.follow_ups, 0, "follow-up");
const STEP_TIMEOUT_MS = 900_000;
// Three counted steps, each a run with a completion check behind it, on one runner queue.
const TASK_TIMEOUT_MS = 720_000;
const TASK_START = "task_start";
const TASK_INPUT = "task_input";
const GOAL_CHECK = "goal";
const PERSON_ACTOR_PREFIX = "steer:";
// The statuses a task rests in once its runs are over.
const FINISHED: readonly string[] = [DURABLE_TASK_STATUS.done, DURABLE_TASK_STATUS.blocked, DURABLE_TASK_STATUS.cancelled, DURABLE_TASK_STATUS.exhausted];
const TasksSchema = z.object({items: z.array(z.object({id: z.uuidv7(), title: z.string().min(1), status: z.string()}))});
const TaskSchema = z.object({context: z.object({
  objective: z.string(), state: z.string(),
  questions: z.array(z.object({name: z.string(), question: z.object({instructions: z.string()})})).min(1),
  usage: z.object({runs: z.number().int(), decision_calls: z.number().int()}),
})});
/** `task_start`'s statements, as the fleet's model wrote them. */
const StartedSchema = z.object({source_turn_id: z.string(), statements: z.array(z.object({id: z.string().min(1), question: z.string().min(1)})).min(1)});

/** The completion checks a turn's run recorded. */
function goalChecks(turn: TurnDetail): string[] {
  return (readDurableTaskActivityHistory(turn.durable_task_activity)?.items ?? [])
    .filter((item) => item.progress.operation === GOAL_CHECK).map((item) => item.kind);
}

test.describe.serial(`prime fleet ${fixture.name}`, {tag: POST_DEPLOY_TAG}, () => {
  test.setTimeout(STEP_TIMEOUT_MS);
  let workspaceId = "";
  let fleet: PrimedFleet | null = null;
  let started: {taskId: string; asked: TurnDetail; statements: z.infer<typeof StartedSchema>["statements"]} | null = null;

  const primed = (): PrimedFleet => {
    if (fleet === null) throw new Error(`${fixture.name} was not created; nothing to walk`);
    return fleet;
  };
  const task = () => {
    if (started === null) throw new Error("the fleet started no task; nothing to read");
    return started;
  };
  const tasksPath = (): string => `/v1/workspaces/${workspaceId}/fleets/${primed().id}/durable_tasks`;
  const tasks = async () => TasksSchema.parse(await observed(JOURNEY_LEG.observe, () => clientFor(PRIME_PERSON).get<unknown>(tasksPath()))).items;
  const detail = async () => TaskSchema.parse(await observed(JOURNEY_LEG.observe, () =>
    clientFor(PRIME_PERSON).get<unknown>(`${tasksPath()}/${task().taskId}`))).context;

  test.beforeAll(async () => {
    test.setTimeout(STEP_TIMEOUT_MS);
    workspaceId = await getDefaultWorkspaceId(PRIME_PERSON);
    fleet = await observed(JOURNEY_LEG.install, () => createPrimeFleet(PRIME_PERSON, workspaceId, fixture));
  });

  test.afterAll(async () => {
    if (fleet) await purgeFixtureFleet(PRIME_PERSON, workspaceId, fleet.id);
  });

  test(proof(0), async ({page}) => {
    await signInAs(page, PRIME_PERSON);
    expect(await tasks(), "a new fleet has no task").toEqual([]);
    // The task's objective is the fixture's own sentence: the first bytes of the message, before what this run adds.
    const asked = await say(page, workspaceId, primed().id, `${objective} Call ${TASK_START} with the target api_only. ` +
      `Its objective is the first sentence of this message: source_start 0 and source_end ${Buffer.byteLength(objective)}.`);
    assertPassed(classifyTerminalTurn(asked));
    expect(asked.actor).toBe(`${PERSON_ACTOR_PREFIX}${fixtureSubject(PRIME_PERSON)}`);
    const call = StartedSchema.parse(callTo(asked, TASK_START));
    expect(call.source_turn_id).toBe(asked.turn_id);
    const listed = await tasks();
    expect(listed, "one message starts one task").toHaveLength(1);
    started = {taskId: entry(fixture, listed, 0, "task").id, asked, statements: call.statements};
  });

  test(proof(1), async () => {
    const {objective: stored, questions} = await detail();
    // The objective is the person's own words: a span of the message that asked.
    expect(stored.length).toBeGreaterThan(0);
    expect(messageOf(task().asked)).toContain(stored);
    expect(questions.map((question) => question.name)).toEqual(task().statements.map((statement) => statement.id));
    for (const [index, statement] of task().statements.entries()) {
      expect(questions[index]?.question.instructions).toContain(statement.question);
    }
  });

  test(proof(2), async () => {
    // The fleet's list holds unfinished work only, so a task's ending is read by its identity.
    const finished = await pollFor(async () => {
      const current = await detail();
      return FINISHED.includes(current.state) ? current : null;
    }, TASK_TIMEOUT_MS);
    if (finished === null) throw new Error(`task ${task().taskId} was still unfinished after ${TASK_TIMEOUT_MS / 1000} s; last state ${(await detail()).state}`);
    const {state, usage} = finished;
    expect(state).toBe(DURABLE_TASK_STATUS.done);
    expect(await tasks(), "a finished task leaves the fleet's unfinished work").toEqual([]);
    expect(usage.runs, "the task ran").toBeGreaterThan(0);
    expect(usage.decision_calls, "a completion check judged it").toBeGreaterThan(0);
    // The task's own runs are turns on the fleet's thread, and each records its completion check.
    const checked = (await threadOf(workspaceId, primed().id)).filter((turn) => goalChecks(turn).length > 0);
    expect(checked.length, "a task run's turn records its completion check").toBeGreaterThan(0);
    expect(checked.map((turn) => turn.turn_id)).not.toContain(task().asked.turn_id);
  });

  test(proof(3), async ({page}) => {
    await signInAs(page, PRIME_PERSON);
    const before = (await detail()).usage.decision_calls;
    const token = uniqueTag();
    const plain = await say(page, workspaceId, primed().id, probeMessage(token));
    assertPassed(classifyTerminalTurn(plain));
    expect(plain.response_text).toContain(probeReply(token));
    expect(toolsCalled(plain)).not.toContain(TASK_START);
    expect(toolsCalled(plain)).not.toContain(TASK_INPUT);
    expect(goalChecks(plain)).toEqual([]);
    expect((await detail()).usage.decision_calls).toBe(before);
    expect(await tasks(), "plain chat starts no task").toEqual([]);
  });
});
