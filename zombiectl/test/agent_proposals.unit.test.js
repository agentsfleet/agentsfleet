import test from "node:test";
import assert from "node:assert/strict";
import { commandAgentProposals } from "../src/commands/agent_proposals.js";
import {
  makeNoop,
  ui,
  AGENT_ID,
} from "./helpers.js";

const PROPOSAL_ID = "0195b4ba-8d3a-7f13-8abc-000000000091";

test("commandAgentProposals lists proposal rows", async () => {
  let calledUrl = null;
  let columns = null;
  let rows = null;
  const deps = {
    request: async (_ctx, url) => {
      calledUrl = url;
      return {
        data: [{
          proposal_id: PROPOSAL_ID,
          trigger_reason: "DECLINING_SCORE",
          config_version_id: "0195b4ba-8d3a-7f13-8abc-000000000092",
          created_at: 1700000000000,
        }],
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
  const code = await commandAgentProposals({ stdout: makeNoop(), stderr: makeNoop(), jsonMode: false }, parsed, AGENT_ID, deps);
  assert.equal(code, 0);
  assert.match(calledUrl, /\/proposals$/);
  assert.equal(columns[0].key, "proposal_id");
  assert.equal(rows[0].proposal_id, PROPOSAL_ID);
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
