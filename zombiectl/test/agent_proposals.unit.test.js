import { test } from "bun:test";
import assert from "node:assert/strict";
import { commandAgentProposals } from "../src/commands/agent_proposals.js";
import {
  makeNoop,
  ui,
  AGENT_ID,
} from "./helpers.js";

const PROPOSAL_ID = "0195b4ba-8d3a-7f13-8abc-000000000091";

test("commandAgentProposals lists proposal rows", async () => {
  const realNow = Date.now;
  Date.now = () => 1700000000000;
  let calledUrl = null;
  let columns = null;
  let rows = null;
  const deps = {
    request: async (_ctx, url) => {
      calledUrl = url;
      return {
        data: [
          {
            proposal_id: PROPOSAL_ID,
            status: "VETO_WINDOW",
            trigger_reason: "DECLINING_SCORE",
            auto_apply_at: 1700003600000,
            config_version_id: "0195b4ba-8d3a-7f13-8abc-000000000092",
            created_at: 1700000000000,
          },
        ],
      };
    },
    apiHeaders: () => ({}),
    printJson: () => {},
    printTable: (_stream, cols, items) => {
      columns = cols;
      rows = items;
    },
    ui,
    writeLine: () => {},
  };

  const parsed = { options: {}, positionals: [AGENT_ID] };
  try {
    const code = await commandAgentProposals({ stdout: makeNoop(), stderr: makeNoop(), jsonMode: false }, parsed, AGENT_ID, deps);
    assert.equal(code, 0);
    assert.match(calledUrl, /\/proposals$/);
    assert.equal(columns[0].key, "proposal_id");
    assert.equal(columns[1].key, "status");
    assert.equal(rows[0].proposal_id, PROPOSAL_ID);
    assert.match(rows[0].action, /Auto-applies in 1h 0m/);
    assert.match(rows[0].action, /agent proposals veto/);
  } finally {
    Date.now = realNow;
  }
});

test("commandAgentProposals approve posts to the approve endpoint", async () => {
  let called = null;
  const deps = {
    request: async (_ctx, url, init) => {
      called = { url, init };
      return { proposal_id: PROPOSAL_ID, status: "APPLIED" };
    },
    apiHeaders: () => ({ authorization: "Bearer t" }),
    printJson: () => {},
    printTable: () => {},
    ui,
    writeLine: () => {},
  };

  const parsed = { options: {}, positionals: [AGENT_ID, "approve", PROPOSAL_ID] };
  const code = await commandAgentProposals({ stdout: makeNoop(), stderr: makeNoop(), jsonMode: false }, parsed, AGENT_ID, deps);
  assert.equal(code, 0);
  assert.match(called.url, /:approve$/);
  assert.equal(called.init.method, "POST");
});

test("commandAgentProposals reject sends optional reason body", async () => {
  let called = null;
  const deps = {
    request: async (_ctx, url, init) => {
      called = { url, init };
      return { proposal_id: PROPOSAL_ID, rejection_reason: "EXPIRED", status: "REJECTED" };
    },
    apiHeaders: () => ({ authorization: "Bearer t" }),
    printJson: () => {},
    printTable: () => {},
    ui,
    writeLine: () => {},
  };

  const parsed = { options: { reason: "needs operator follow-up" }, positionals: [AGENT_ID, "reject", PROPOSAL_ID] };
  const code = await commandAgentProposals({ stdout: makeNoop(), stderr: makeNoop(), jsonMode: false }, parsed, AGENT_ID, deps);
  assert.equal(code, 0);
  assert.match(called.url, /:reject$/);
  assert.equal(JSON.parse(called.init.body).reason, "needs operator follow-up");
});

test("commandAgentProposals veto sends optional reason body", async () => {
  let called = null;
  const deps = {
    request: async (_ctx, url, init) => {
      called = { url, init };
      return { proposal_id: PROPOSAL_ID, rejection_reason: "operator pause", status: "VETOED" };
    },
    apiHeaders: () => ({ authorization: "Bearer t" }),
    printJson: () => {},
    printTable: () => {},
    ui,
    writeLine: () => {},
  };

  const parsed = { options: { reason: "operator pause" }, positionals: [AGENT_ID, "veto", PROPOSAL_ID] };
  const code = await commandAgentProposals({ stdout: makeNoop(), stderr: makeNoop(), jsonMode: false }, parsed, AGENT_ID, deps);
  assert.equal(code, 0);
  assert.match(called.url, /:veto$/);
  assert.equal(JSON.parse(called.init.body).reason, "operator pause");
});

test("commandAgentProposals json mode preserves proposal response shape", async () => {
  let jsonValue = null;
  const deps = {
    request: async () => ({
      data: [{
        proposal_id: PROPOSAL_ID,
        status: "VETO_WINDOW",
        approval_mode: "AUTO",
        auto_apply_at: 1700003600000,
        trigger_reason: "DECLINING_SCORE",
        config_version_id: "0195b4ba-8d3a-7f13-8abc-000000000092",
        created_at: 1700000000000,
        updated_at: 1700000000001,
      }],
    }),
    apiHeaders: () => ({}),
    printJson: (_stream, value) => { jsonValue = value; },
    printTable: () => {},
    ui,
    writeLine: () => {},
  };

  const parsed = { options: {}, positionals: [AGENT_ID] };
  const code = await commandAgentProposals({ stdout: makeNoop(), stderr: makeNoop(), jsonMode: true }, parsed, AGENT_ID, deps);
  assert.equal(code, 0);
  assert.equal(jsonValue.data[0].status, "VETO_WINDOW");
  assert.equal(jsonValue.data[0].approval_mode, "AUTO");
  assert.equal(jsonValue.data[0].auto_apply_at, 1700003600000);
});

test("commandAgentProposals rejects missing proposal id for decision commands", async () => {
  const stderr = [];
  const deps = {
    request: async () => ({ data: [] }),
    apiHeaders: () => ({}),
    printJson: () => {},
    printTable: () => {},
    ui,
    writeLine: (_stream, line) => { stderr.push(line); },
  };

  const parsed = { options: {}, positionals: [AGENT_ID, "approve"] };
  const code = await commandAgentProposals({ stdout: makeNoop(), stderr: makeNoop(), jsonMode: false }, parsed, AGENT_ID, deps);
  assert.equal(code, 2);
  assert.match(stderr.join("\n"), /requires <proposal-id>/);
});
