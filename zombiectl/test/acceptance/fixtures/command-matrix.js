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

// Per-row fields:
//   args      — argv passed to the CLI (always includes --json).
//   label     — human label for test naming (defaults to `args.join(" ")`).
//   requiredKey — top-level key the JSON envelope MUST carry on success.
//                 Matrix-driven assertion replaces the spec's pinned
//                 `jsonShape` map (which drifted from the CLI's actual
//                 server-passthrough shape per command).
//   isList    — list command. itemsKey names the array field whose
//                length the §4b' / §5b' empty-list sweep inspects.
export const READ_ONLY_COMMANDS = [
  { args: ["doctor", "--json"], requiredKey: "checks" },
  { args: ["workspace", "list", "--json"], isList: true, itemsKey: "workspaces" },
  { args: ["workspace", "show", "--json"], requiredKey: "workspace_id" },
  { args: ["agent", "list", "--json"], isList: true, itemsKey: "items" },
  { args: ["tenant", "provider", "show", "--json"], requiredKey: "provider_mode" },
  { args: ["billing", "show", "--json"], requiredKey: "balance" },
  { args: ["list", "--json"], isList: true, itemsKey: "items", label: "zombie list" },
];

// Read-only commands scoped to a live zombie_id. The spec interpolates
// the §4a-installed zombieId via `--zombie <id>` before running. Kept
// separate from READ_ONLY_COMMANDS (which is workspace-scoped) because
// `grant list` requires `--zombie <id>`; the §4b read-only sweep cannot
// thread fixture state into a static argv.
export const PER_ZOMBIE_READ_ONLY_COMMANDS = [
  { argsHead: ["grant", "list"], isList: true, itemsKey: "items", group: "grant" },
];

// Per-row flags:
//   apiHits  — `true` iff the CLI dispatches to the live API on a
//              syntactically-valid identifier; `false` for local-only
//              mutators (workspace use/delete). §4c1's "valid-format
//              nonexistent" sweep only iterates rows with `apiHits: true`.
//   validatesClient — `true` iff the handler runs `validateRequiredId`
//              before any dispatch. §4c2's "no-network on invalid-format"
//              invariant only fires for these rows today; other rows are
//              surfaced as Discovery (handlers do not validate IDs
//              client-side and would stress the API).
//   expectedErrorCode — server-side UZ-* code emitted on not-found
//              (only meaningful when `apiHits: true`). Codes verified
//              against zombiectl/../src/errors/error_registry.zig at
//              the time of writing — kept in sync with §4c1.
//   clientRejectCode — CLI-emitted error code when local validation /
//              local lookup rejects the request (apiHits: false rows).
export const REQUIRES_IDENTIFIER = [
  // status is the only zombie verb that does NOT run validateRequiredId
  // (it accepts an optional positional and falls back to workspace-wide).
  { args: ["status"], expectedErrorCode: "UZ-ZMB-009", argName: "zombie_id", apiHits: true, validatesClient: false },
  // kill/stop/resume/logs and grant/agent delete all run validateRequiredId
  // — §4c2 sweep relies on validatesClient: true to fire the no-network
  // invariant against an invalid-format id sample.
  { args: ["kill"], expectedErrorCode: "UZ-ZMB-009", argName: "zombie_id", apiHits: true, validatesClient: true },
  { args: ["stop"], expectedErrorCode: "UZ-ZMB-009", argName: "zombie_id", apiHits: true, validatesClient: true },
  { args: ["resume"], expectedErrorCode: "UZ-ZMB-009", argName: "zombie_id", apiHits: true, validatesClient: true },
  { args: ["logs"], expectedErrorCode: "UZ-ZMB-009", argName: "zombie_id", apiHits: true, validatesClient: true },
  { args: ["workspace", "use"], argName: "workspace_id", apiHits: false, validatesClient: true, clientRejectCode: "UNKNOWN_WORKSPACE" },
  { args: ["workspace", "delete"], argName: "workspace_id", apiHits: false, validatesClient: true, clientRejectCode: null },
  { args: ["agent", "delete"], expectedErrorCode: "UZ-AGENT-001", argName: "key_id", apiHits: true, validatesClient: true },
  { args: ["grant", "delete"], expectedErrorCode: "UZ-GRANT-001", argName: "grant_id", apiHits: true, validatesClient: true },
];

// Commands whose first positional is `<required>` in cli-tree and so
// produce commander's "missing required argument" rejection (matched by
// `expectMissingArg`'s /missing|required|usage|expected/ regex).
//
// `status [zombie_id]` and `logs [zombie_id]` are optional positionals
// — `status` bare exits 0 (workspace-wide fallback) and `logs` bare
// exits 2 with a domain-specific stem ("logs requires --zombie <id>")
// that the generic missing-arg regex does not match. They are
// exercised in §4a's lifecycle walk with a real zombieId instead.
export const REQUIRES_POSITIONAL_ARG = [
  { args: ["workspace", "use"], missingArgName: "workspace_id" },
  { args: ["workspace", "delete"], missingArgName: "workspace_id" },
  { args: ["agent", "delete"], missingArgName: "key_id" },
  { args: ["grant", "delete"], missingArgName: "grant_id" },
  { args: ["kill"], missingArgName: "zombie_id" },
  { args: ["stop"], missingArgName: "zombie_id" },
  { args: ["resume"], missingArgName: "zombie_id" },
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
