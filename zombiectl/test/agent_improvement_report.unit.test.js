import test from "node:test";
import assert from "node:assert/strict";
import { commandAgentImprovementReport } from "../src/commands/agent_improvement_report.js";
import {
  makeNoop,
  ui, ApiError,
  AGENT_ID,
} from "./helpers.js";

const SAMPLE_REPORT = {
  agent_id: AGENT_ID,
  trust_level: "UNEARNED",
  improvement_stalled_warning: true,
  proposals_generated: 4,
  proposals_approved: 1,
  proposals_vetoed: 1,
  proposals_rejected: 1,
  proposals_applied: 2,
  avg_score_delta_per_applied_change: -7.5,
  current_tier: "Silver",
  baseline_tier: "Gold",
};

test("commandAgentImprovementReport calls GET /v1/agents/{agent_id}/improvement-report", async () => {
  let calledUrl = null;
  const deps = {
    request: async (_ctx, url) => { calledUrl = url; return SAMPLE_REPORT; },
    apiHeaders: () => ({}),
    printJson: () => {},
    printKeyValue: () => {},
  };
  const code = await commandAgentImprovementReport({ stdout: makeNoop(), stderr: makeNoop(), jsonMode: false }, { options: {}, positionals: [] }, AGENT_ID, deps);
  assert.equal(code, 0);
  assert.match(calledUrl, new RegExp(`${AGENT_ID}/improvement-report$`));
});

test("commandAgentImprovementReport human mode prints report fields", async () => {
  let kvData = null;
  const deps = {
    request: async () => SAMPLE_REPORT,
    apiHeaders: () => ({}),
    printJson: () => {},
    printKeyValue: (_stream, value) => { kvData = value; },
  };
  await commandAgentImprovementReport({ stdout: makeNoop(), stderr: makeNoop(), jsonMode: false }, { options: {}, positionals: [] }, AGENT_ID, deps);
  assert.equal(kvData.improvement_stalled_warning, true);
  assert.equal(kvData.proposals_applied, 2);
  assert.equal(kvData.current_tier, "Silver");
});

test("commandAgentImprovementReport json mode outputs raw response", async () => {
  let printed = null;
  const deps = {
    request: async () => SAMPLE_REPORT,
    apiHeaders: () => ({}),
    printJson: (_stream, value) => { printed = value; },
    printKeyValue: () => {},
  };
  await commandAgentImprovementReport({ stdout: makeNoop(), stderr: makeNoop(), jsonMode: true }, { options: {}, positionals: [] }, AGENT_ID, deps);
  assert.deepEqual(printed, SAMPLE_REPORT);
});

test("commandAgentImprovementReport propagates ApiError", async () => {
  const deps = {
    request: async () => { throw new ApiError("not found", { status: 404, code: "UZ-AGENT-001" }); },
    apiHeaders: () => ({}),
    printJson: () => {},
    printKeyValue: () => {},
  };
  await assert.rejects(
    () => commandAgentImprovementReport({ stdout: makeNoop(), stderr: makeNoop(), jsonMode: false }, { options: {}, positionals: [] }, AGENT_ID, deps),
    (err) => err instanceof ApiError && err.status === 404,
  );
});
