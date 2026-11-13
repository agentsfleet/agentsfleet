// Every regrouped command, with the request sequence it issues.
//
// Each row's `calls` was recorded once by running the command's previous
// root-level spelling against a stub that answers every request with a 404
// problem, at the commit before the regroup; the new spelling must issue the
// same sequence, which is what proves it reaches the same handler. The stub
// stops most commands after their first request, so a row records which
// handler ran, not a full happy path.

export const WORKSPACE_ID = "0192a3b4-c5d6-7e8f-9012-345678901230";
export const FLEET_ID = "0192a3b4-c5d6-7e8f-9012-345678901234";
export const GATE_ID = "0192a3b4-c5d6-7e8f-9012-345678901235";
export const SCHEDULE_ID = "0192a3b4-c5d6-7e8f-9012-345678901236";
export const GRANT_ID = "0192a3b4-c5d6-7e8f-9012-345678901237";
export const API_KEY_ID = "0192a3b4-c5d6-7e8f-9012-345678901238";

const WS = `/v1/workspaces/${WORKSPACE_ID}`;
const FLEET = `${WS}/fleets/${FLEET_ID}`;

export interface ParityRow {
  readonly argv: ReadonlyArray<string>;
  readonly calls: ReadonlyArray<string>;
}

export const COMMAND_PARITY: ReadonlyArray<ParityRow> = [
  { argv: ["fleet", "list"], calls: [`GET ${WS}/fleets`] },
  { argv: ["fleet", "status"], calls: [`GET ${WS}/fleets`] },
  // New in §6, so recorded on this branch rather than at the base.
  { argv: ["fleet", "activate", FLEET_ID], calls: [`GET ${FLEET}`] },
  { argv: ["fleet", "archive", FLEET_ID], calls: [`GET ${FLEET}`] },
  { argv: ["fleet", "delete", FLEET_ID], calls: [`GET ${FLEET}`] },
  { argv: ["fleet", "steer", FLEET_ID, "check the release"], calls: [`POST ${FLEET}/messages`] },
  { argv: ["fleet", "logs", "--fleet", FLEET_ID], calls: [`GET ${FLEET}/events`] },
  { argv: ["fleet", "events", FLEET_ID], calls: [`GET ${FLEET}/events`] },
  { argv: ["fleet", "approvals", "list"], calls: [`GET ${WS}/approvals`] },
  { argv: ["fleet", "approvals", "show", GATE_ID], calls: [`GET ${WS}/approvals/${GATE_ID}`] },
  { argv: ["fleet", "approvals", "approve", GATE_ID], calls: [`POST ${WS}/approvals/${GATE_ID}/approve`] },
  { argv: ["fleet", "approvals", "deny", GATE_ID], calls: [`POST ${WS}/approvals/${GATE_ID}/deny`] },
  { argv: ["fleet", "schedule", "list", FLEET_ID], calls: [`GET ${FLEET}/schedules`] },
  { argv: ["fleet", "schedule", "show", FLEET_ID, SCHEDULE_ID], calls: [`GET ${FLEET}/schedules/${SCHEDULE_ID}`] },
  { argv: ["fleet", "schedule", "create", FLEET_ID, "--cron", "0 9 * * *", "--message", "standup"], calls: [`POST ${FLEET}/schedules`] },
  { argv: ["fleet", "schedule", "update", FLEET_ID, SCHEDULE_ID, "--message", "standup"], calls: [`GET ${FLEET}/schedules/${SCHEDULE_ID}`] },
  { argv: ["fleet", "schedule", "delete", FLEET_ID, SCHEDULE_ID], calls: [`DELETE ${FLEET}/schedules/${SCHEDULE_ID}`] },
  { argv: ["fleet", "schedule", "sync", FLEET_ID, SCHEDULE_ID], calls: [`POST ${FLEET}/schedules/${SCHEDULE_ID}/sync`] },
  { argv: ["fleet", "memory", "list", "--fleet", FLEET_ID], calls: [`GET ${FLEET}/memories`] },
  { argv: ["fleet", "memory", "search", "--fleet", FLEET_ID, "release"], calls: [`GET ${FLEET}/memories`] },
  { argv: ["fleet", "grants", "list", "--fleet", FLEET_ID], calls: [`GET ${FLEET}/integration-grants`] },
  { argv: ["fleet", "grants", "revoke", GRANT_ID, "--fleet", FLEET_ID], calls: [`DELETE ${FLEET}/integration-grants/${GRANT_ID}`] },
  { argv: ["models", "list"], calls: [`GET /v1/models`] },
  { argv: ["models", "list", "--provider", "anthropic"], calls: [`GET /v1/models`] },
  { argv: ["models", "current"], calls: [`GET /v1/tenants/me/provider`] },
  { argv: ["models", "use", "--secret", "my-key"], calls: [`PUT /v1/tenants/me/provider`] },
  { argv: ["models", "reset"], calls: [`DELETE /v1/tenants/me/provider`] },
  { argv: ["secrets", "list"], calls: [`GET ${WS}/secrets`] },
  { argv: ["secrets", "show", "my-key"], calls: [`GET ${WS}/secrets`] },
  { argv: ["secrets", "create", "my-key", "--data", "{\"api_key\":\"x\"}"], calls: [`POST ${WS}/secrets`] },
  { argv: ["secrets", "update", "my-key", "--data", "{\"api_key\":\"x\"}"], calls: [`PUT ${WS}/secrets/my-key`] },
  { argv: ["secrets", "delete", "my-key"], calls: [`DELETE ${WS}/secrets/my-key`] },
  { argv: ["connections", "list"], calls: [`GET ${WS}/connectors`] },
  { argv: ["connections", "show", "github"], calls: [`GET ${WS}/connectors`] },
  { argv: ["api-keys", "list"], calls: [`GET /v1/api-keys`] },
  { argv: ["api-keys", "create", "--name", "ci"], calls: [`POST /v1/api-keys`] },
  { argv: ["api-keys", "revoke", API_KEY_ID], calls: [`PATCH /v1/api-keys/${API_KEY_ID}`] },
  { argv: ["api-keys", "delete", API_KEY_ID], calls: [`DELETE /v1/api-keys/${API_KEY_ID}`] },
  { argv: ["usage", "show"], calls: [`GET /v1/tenants/me/billing`, `GET /v1/tenants/me/billing/charges`] },
];
