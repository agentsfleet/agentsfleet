import type { Definition } from "@/lib/api/draft-types";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { afterEach, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { SetupQuestions } from "@/components/domain/chat/SetupQuestions";
import { FleetSetup } from "@/components/domain/chat/FleetSetup";
import { readDecisionPolicy } from "@/lib/api/decision-policy";
import { DraftClient } from "@/lib/api/drafts";
import { EMPTY_DRAFT_VIEW } from "@/lib/api/draft-state";
import { DRAFT, FLEET, REVIEW, WORKSPACE } from "@/tests/helpers/draft-fixtures";

const FIXTURE: unknown = JSON.parse(readFileSync(resolve(process.cwd(), "../../../tests/fixtures/fleets/goal-repair/decision-policy.json"), "utf8"));
const QUESTION = "Does the patch fix the task's accepted Linkwarden defect?";
const YES = "Current code and regression evidence demonstrate the accepted behavior.";
const NO = "The defect remains, scope changed, or evidence is missing or contradictory.";

afterEach(cleanup);

it("shows editable success criteria without exposing routing or model terminology", () => {
  render(<SetupQuestions definition={definition(FIXTURE)} />);
  expect(screen.getByRole("heading", {name: "What counts as done?"})).toBeTruthy();
  expect(screen.getByText(`1. ${QUESTION}`)).toBeTruthy();
  expect(screen.getByText(YES)).toBeTruthy();
  expect(screen.getByText(NO)).toBeTruthy();
  expect(screen.getByText(/Each save creates a new revision/)).toBeTruthy();
  expect(screen.getByText("Work already started keeps its saved success criteria.")).toBeTruthy();
  expect(screen.queryByText(/Noul|probability|CONTINUE|BLOCKED/)).toBeNull();
  expect(screen.queryByRole("button")).toBeNull();
  expect(screen.queryByText("Change the requested approach for an existing repair.")).toBeNull();
  expect(screen.queryByText("Which trusted task matches the newest request?")).toBeNull();
  expect(screen.queryByText("A recorded external dependency must finish before useful action can continue.")).toBeNull();
});

it("preserves ordinary fleets without adding decision defaults", () => {
  render(<SetupQuestions definition={{...REVIEW.effective.definition, config: {"x-agentsfleet": {}}}} />);
  expect(screen.queryByRole("region", {name: "Success criteria"})).toBeNull();
});

it("keeps malformed, missing and unsupported criteria visibly unconfirmed", () => {
  const policy = parsedFixture();
  for (const value of [null, {}, {routing: {}},
    {...policy, completion: {...policy.completion, statements: [policy.completion.statements[0], policy.completion.statements[0]]}},
    {...policy, completion: {...policy.completion, next_action: {question: "Choose", criteria: {CONTINUE: "Go", WAIT: "Wait", DONE: "Done", PUBLISH: "Any target"}}}},
  ]) {
    const result = render(<SetupQuestions definition={definition(value)} />);
    expect(screen.getByText("Success criteria unavailable")).toBeTruthy();
    expect(screen.queryByText(`1. ${QUESTION}`)).toBeNull();
    result.unmount();
  }
});

it("renders revised criteria without mutating or falsely replacing the accepted wording", () => {
  const policy = parsedFixture();
  const changed = "A regression test reproduces the defect and passes for the exact proposed patch.";
  const revised = {...policy, completion: {...policy.completion, statements: policy.completion.statements.map((statement, index) =>
    index === 0 ? {...statement, criteria: {...statement.criteria, true: changed}} : statement)}};
  const view = render(<SetupQuestions definition={definition(FIXTURE)} />);
  expect(screen.getByText(YES)).toBeTruthy();
  view.rerender(<SetupQuestions definition={definition(revised)} />);
  expect(screen.getByText(changed)).toBeTruthy();
  expect(screen.queryByText(YES)).toBeNull();
  view.rerender(<SetupQuestions definition={definition(FIXTURE)} />);
  expect(screen.getByText(YES)).toBeTruthy();
  expect(screen.queryByText(changed)).toBeNull();
});

it("shows proposed and saved questions in the primary setup screen using the matching revision", () => {
  const candidate = DRAFT.candidate;
  if (!candidate) throw new Error("setup fixture needs a candidate");
  const policy = parsedFixture();
  const changed = "Proposed criterion awaiting the owner's Apply.";
  const proposedPolicy = {...policy, completion: {...policy.completion,
    statements: policy.completion.statements.map((statement, index) => index === 0
      ? {...statement, criteria: {...statement.criteria, true: changed}} : statement)}};
  const client = new DraftClient({account: "owner", workspace: WORKSPACE, subject: "creator"});
  const view = {...EMPTY_DRAFT_VIEW,
    draft: {...DRAFT, candidate: {...candidate, definition: definition(proposedPolicy)}},
    fleet: {...FLEET, setup_revision: "7", definition: definition(FIXTURE)},
  };
  try {
    const rendered = render(<FleetSetup client={client} view={view} workspace={WORKSPACE} />);
    expect(screen.getByText("Proposed questions · saved setup R7")).toBeTruthy();
    expect(screen.getByText(`1. ${QUESTION}`)).toBeTruthy();
    expect(screen.getByText(changed)).toBeTruthy();
    expect(screen.queryByText(YES)).toBeNull();
    rendered.rerender(<FleetSetup client={client} view={{...view, draft: null}} workspace={WORKSPACE} />);
    expect(screen.getByText("Questions in saved setup R7")).toBeTruthy();
    expect(screen.queryByText("Proposed questions · saved setup R7")).toBeNull();
    expect(screen.getByText(`1. ${QUESTION}`)).toBeTruthy();
    expect(screen.getByText(YES)).toBeTruthy();
    expect(screen.queryByText(changed)).toBeNull();
    rendered.rerender(<FleetSetup client={client} view={{...view, inaccessible: true}} workspace={WORKSPACE} />);
    expect(screen.queryByText(`1. ${QUESTION}`)).toBeNull();
  } finally { client.dispose(); }
});

it("preserves valid Unicode wording and rejects text above the wire byte ceiling", () => {
  const policy = parsedFixture();
  const wording = "🙂".repeat(120);
  const withQuestion = (question: string) => ({...policy, completion: {...policy.completion,
    statements: policy.completion.statements.map((statement, index) => index === 0 ? {...statement, question} : statement)}});
  const view = render(<SetupQuestions definition={definition(withQuestion(wording))} />);
  expect(screen.getByText(`1. ${wording}`)).toBeTruthy();
  view.rerender(<SetupQuestions definition={definition(withQuestion("界".repeat(200)))} />);
  expect(screen.getByText("Success criteria unavailable")).toBeTruthy();
  expect(screen.queryByText(`1. ${wording}`)).toBeNull();
});

function definition(policy: unknown): Definition {
  return {...REVIEW.effective.definition, config: {"x-agentsfleet": {decision_policy: policy}}};
}

function parsedFixture() {
  const result = readDecisionPolicy(definition(FIXTURE).config);
  if (result.kind !== "ready") throw new Error("shared policy fixture must be readable");
  return result.policy;
}
