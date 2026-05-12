/**
 * Single source of truth for the command-group enumeration the unauth +
 * ZOMBIE_TOKEN acceptance suites iterate over.
 *
 * RULE UFS: every "list of commands" literal lives here once. Specs read
 * from these exports; nothing inlines a command-string list.
 *
 * If a new command surface lands in `zombiectl/src/program/routes.js`,
 * the implementing agent of THAT change extends the relevant table here
 * and the spec sweeps pick it up automatically.
 */

export const COMMAND_GROUPS = [
  "workspace",
  "agent",
  "grant",
  "tenant",
  "billing",
  "zombie",
];

export const READ_ONLY_COMMANDS = [
  { args: ["doctor", "--json"], jsonShape: { object: "doctor" } },
  { args: ["workspace", "list", "--json"], jsonShape: { items: "array" }, isList: true },
  { args: ["workspace", "show", "--json"], jsonShape: { workspace_id: "string" } },
  { args: ["agent", "list", "--json"], jsonShape: { items: "array" }, isList: true },
  { args: ["grant", "list", "--json"], jsonShape: { items: "array" }, isList: true },
  { args: ["tenant", "provider", "show", "--json"], jsonShape: { provider_mode: "string" } },
  { args: ["billing", "show", "--json"], jsonShape: { balance: "any" } },
  { args: ["list", "--json"], jsonShape: { items: "array" }, isList: true, label: "zombie list" },
];

export const REQUIRES_IDENTIFIER = [
  { args: ["status"], expectedErrorCode: "UZ-ZOMBIE-NOT-FOUND", argName: "zombie_id" },
  { args: ["kill"], expectedErrorCode: "UZ-ZOMBIE-NOT-FOUND", argName: "zombie_id" },
  { args: ["stop"], expectedErrorCode: "UZ-ZOMBIE-NOT-FOUND", argName: "zombie_id" },
  { args: ["resume"], expectedErrorCode: "UZ-ZOMBIE-NOT-FOUND", argName: "zombie_id" },
  { args: ["logs"], expectedErrorCode: "UZ-ZOMBIE-NOT-FOUND", argName: "zombie_id" },
  { args: ["workspace", "use"], expectedErrorCode: "UZ-WORKSPACE-NOT-FOUND", argName: "workspace_id" },
  { args: ["workspace", "delete"], expectedErrorCode: "UZ-WORKSPACE-NOT-FOUND", argName: "workspace_id" },
  { args: ["agent", "delete"], expectedErrorCode: "UZ-AGENT-NOT-FOUND", argName: "key_id" },
  { args: ["grant", "delete"], expectedErrorCode: "UZ-GRANT-NOT-FOUND", argName: "grant_id" },
];

export const REQUIRES_POSITIONAL_ARG = [
  { args: ["workspace", "use"], missingArgName: "workspace_id" },
  { args: ["workspace", "delete"], missingArgName: "workspace_id" },
  { args: ["agent", "delete"], missingArgName: "key_id" },
  { args: ["grant", "delete"], missingArgName: "grant_id" },
  { args: ["status"], missingArgName: "zombie_id" },
  { args: ["kill"], missingArgName: "zombie_id" },
  { args: ["stop"], missingArgName: "zombie_id" },
  { args: ["resume"], missingArgName: "zombie_id" },
  { args: ["logs"], missingArgName: "zombie_id" },
];

export const INVALID_ID_SAMPLES = [
  "not-a-uuid",
  "foo",
  "abc def",
  "---",
];

/**
 * Per-list-command empty-collection conventions. Stem matches the current
 * `ui.info(...)` output the existing CLI emits; the spec sweeps assert the
 * stem appears in non-JSON mode and `{items: [], total: 0}` in JSON mode.
 *
 * Stems read with substring match — if the CLI tightens its wording, the
 * test still passes as long as the kebab/space stem is present.
 */
export const EMPTY_LIST_CONVENTIONS = {
  "workspace list": "no workspaces",
  "agent list": "no agent",
  "grant list": "no grant",
  "list": "no zombies",
};

export const AUTH_REQUIRED_REPRESENTATIVE = [
  ["doctor"],
  ["workspace", "list"],
  ["billing", "show"],
  ["list"],
];
