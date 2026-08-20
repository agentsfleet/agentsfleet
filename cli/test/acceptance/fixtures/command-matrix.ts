/**
 * Single source of truth for the command-group enumeration the unauth +
 * authenticated acceptance suites iterate over.
 *
 * RULE UFS: every "list of commands" literal lives here once. Specs read
 * from these exports; nothing inlines a command-string list.
 *
 * If a new command surface lands in `cli/src/program/routes.js`,
 * the implementing fleet of THAT change extends the relevant table here
 * and the spec sweeps pick it up automatically.
 */

// A well-formed uuidv7 for rows that need a syntactically valid id to reach
// the rejection under test. Declared before first use — these arrays are
// evaluated at module load, so a bottom-declared const would land in the
// temporal dead zone.
export const EXAMPLE_FLEET_ID = "0192a3b4-c5d6-7e8f-9012-345678901234";

export const COMMAND_GROUPS: ReadonlyArray<string> = [
  "workspace",
  "api-key",
  "connector",
  "grant",
  "tenant",
  "billing",
  "fleet",
  "memory",
];

export interface ReadOnlyCommandRow {
  readonly args: ReadonlyArray<string>;
  readonly label?: string;
  readonly requiredKey?: string;
  readonly isList?: boolean;
  readonly itemsKey?: string;
}

// Per-row fields:
//   args      — argv passed to the CLI (always includes --json).
//   label     — human label for test naming (defaults to `args.join(" ")`).
//   requiredKey — top-level key the JSON envelope MUST carry on success.
//                 Matrix-driven assertion replaces the spec's pinned
//                 `jsonShape` map (which drifted from the CLI's actual
//                 server-passthrough shape per command).
//   isList    — list command. itemsKey names the array field whose
//                length the §4b' / §5b' empty-list sweep inspects.
export const READ_ONLY_COMMANDS: ReadonlyArray<ReadOnlyCommandRow> = [
  { args: ["doctor", "--json"], requiredKey: "checks" },
  { args: ["workspace", "list", "--json"], isList: true, itemsKey: "workspaces" },
  { args: ["workspace", "show", "--json"], requiredKey: "workspace_id" },
  { args: ["api-key", "list", "--json"], isList: true, itemsKey: "items" },
  { args: ["connector", "list", "--json"], label: "connector list" },
  { args: ["tenant", "provider", "show", "--json"], requiredKey: "mode" },
  { args: ["billing", "show", "--json"], requiredKey: "balance_nanos" },
  { args: ["list", "--json"], isList: true, itemsKey: "items", label: "fleet list" },
];

export interface PerFleetReadOnlyCommandRow {
  readonly argsHead: ReadonlyArray<string>;
  readonly isList?: boolean;
  readonly itemsKey?: string;
  readonly requiredKey?: string;
  readonly group?: string;
}

// Read-only commands scoped to a live fleet_id. The spec interpolates
// the §4a-installed fleetId via `--fleet <id>` before running. Kept
// separate from READ_ONLY_COMMANDS (which is workspace-scoped) because
// `grant list` requires `--fleet <id>`; the §4b read-only sweep cannot
// thread fixture state into a static argv.
export const PER_AGENTSFLEET_READ_ONLY_COMMANDS: ReadonlyArray<PerFleetReadOnlyCommandRow> = [
  { argsHead: ["grant", "list"], isList: true, itemsKey: "items", group: "grant" },
  { argsHead: ["memory", "list"], isList: true, itemsKey: "items", group: "memory" },
];

export interface RequiresIdentifierRow {
  readonly args: ReadonlyArray<string>;
  readonly argName: string;
  readonly apiHits: boolean;
  readonly validatesClient: boolean;
  readonly expectedErrorCode?: string;
  readonly clientRejectCode?: string | null;
}

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
//              against agentsfleet/../src/errors/error_registry.zig at
//              the time of writing — kept in sync with §4c1.
//   clientRejectCode — CLI-emitted error code when local validation /
//              local lookup rejects the request (apiHits: false rows).
export const REQUIRES_IDENTIFIER: ReadonlyArray<RequiresIdentifierRow> = [
  // status accepts an optional positional and currently falls back to a
  // workspace-wide list response, so it is not a by-ID not-found probe.
  { args: ["status"], expectedErrorCode: "UZ-AGT-009", argName: "fleet_id", apiHits: false, validatesClient: false },
  // kill/stop/resume/logs and grant/fleet delete all run validateRequiredId
  // — §4c2 sweep relies on validatesClient: true to fire the no-network
  // invariant against an invalid-format id sample.
  { args: ["kill"], expectedErrorCode: "UZ-AGT-009", argName: "fleet_id", apiHits: false, validatesClient: true },
  { args: ["stop"], expectedErrorCode: "UZ-AGT-009", argName: "fleet_id", apiHits: false, validatesClient: true },
  { args: ["resume"], expectedErrorCode: "UZ-AGT-009", argName: "fleet_id", apiHits: false, validatesClient: true },
  { args: ["logs"], expectedErrorCode: "UZ-AGT-009", argName: "fleet_id", apiHits: false, validatesClient: true },
  { args: ["workspace", "use"], argName: "workspace_id", apiHits: false, validatesClient: true, clientRejectCode: "UNKNOWN_WORKSPACE" },
  { args: ["workspace", "delete"], argName: "workspace_id", apiHits: false, validatesClient: true, clientRejectCode: null },
  // api-key delete validates its positional through commander's parseIdOption,
  // not the handler's validateRequiredId, so its rejection text omits the
  // `invalid <name>: ` stem the client-side sweep below asserts — hence
  // validatesClient: false. The server-side probe still applies: a well-formed
  // but unknown key answers UZ-APIKEY-003 (ERR_APIKEY_NOT_FOUND).
  { args: ["api-key", "delete"], expectedErrorCode: "UZ-APIKEY-003", argName: "api_key_id", apiHits: true, validatesClient: false },
  // grant delete also requires --fleet <id>, so the generic single-ID
  // matrix cannot exercise it without a live fleet fixture.
  { args: ["grant", "delete"], expectedErrorCode: "UZ-GRANT-001", argName: "grant_id", apiHits: false, validatesClient: false },
];

export interface RequiresPositionalArgRow {
  readonly args: ReadonlyArray<string>;
  readonly missingArgName: string;
  // The token the rejection actually names, when it differs from the
  // positional. commander validates required OPTIONS before positionals, so
  // a bare `schedule add` is told about --cron first.
  readonly reportedToken?: string;
}

// Commands whose first positional is `<required>` in cli-tree and so
// produce commander's "missing required argument" rejection (matched by
// `expectMissingArg`'s /missing|required|usage|expected/ regex).
//
// `logs [fleet_id]` has an optional positional. Bare `logs` exits 2 with a
// domain-specific stem that the generic missing-argument check does not match.
export const REQUIRES_POSITIONAL_ARG: ReadonlyArray<RequiresPositionalArgRow> = [
  { args: ["workspace", "create"], missingArgName: "name" },
  { args: ["workspace", "use"], missingArgName: "workspace_id" },
  { args: ["workspace", "delete"], missingArgName: "workspace_id" },
  { args: ["api-key", "revoke"], missingArgName: "api_key_id" },
  { args: ["api-key", "delete"], missingArgName: "api_key_id" },
  { args: ["grant", "delete"], missingArgName: "grant_id" },
  { args: ["connector", "status"], missingArgName: "provider" },
  { args: ["kill"], missingArgName: "fleet_id" },
  { args: ["stop"], missingArgName: "fleet_id" },
  { args: ["resume"], missingArgName: "fleet_id" },
  { args: ["delete"], missingArgName: "fleet_id" },
  { args: ["events"], missingArgName: "fleet_id" },
  { args: ["steer"], missingArgName: "fleet_id" },
  { args: ["fleet", "update"], missingArgName: "fleet_id" },
  { args: ["memory", "search"], missingArgName: "query" },
  { args: ["secret", "create"], missingArgName: "name" },
  { args: ["secret", "update"], missingArgName: "name" },
  { args: ["secret", "show"], missingArgName: "name" },
  { args: ["secret", "delete"], missingArgName: "name" },
  { args: ["schedule", "add"], missingArgName: "fleet_id", reportedToken: "--cron" },
  { args: ["schedule", "list"], missingArgName: "fleet_id" },
  { args: ["schedule", "update"], missingArgName: "fleet_id" },
  { args: ["schedule", "rm"], missingArgName: "fleet_id" },
  { args: ["schedule", "status"], missingArgName: "fleet_id" },
  { args: ["schedule", "sync"], missingArgName: "fleet_id" },
];

// Every command node that owns subcommands. Invoked bare each must print its
// help on STDOUT and exit 0 — the body has to survive a pipe, which it did
// not before M171 (commander routes a group's bare invocation through
// `help({ error: true })`, i.e. stderr).
export const GROUP_NODES: ReadonlyArray<ReadonlyArray<string>> = [
  ["auth"],
  ["workspace"],
  ["api-key"],
  ["connector"],
  ["grant"],
  ["tenant"],
  ["tenant", "provider"],
  ["billing"],
  ["fleet"],
  ["secret"],
  ["schedule"],
  ["memory"],
];

// A value-taking flag with its value omitted. commander raises
// optionMissingArgument at parse time, so no state or network is involved.
export const MISSING_OPTION_VALUE: ReadonlyArray<ReadonlyArray<string>> = [
  ["logs", "--fleet"],
  ["list", "--limit"],
  ["billing", "show", "--limit"],
  ["memory", "list", "--fleet"],
  ["grant", "list", "--fleet"],
  ["connector", "list", "--workspace"],
];

// A required option omitted entirely — commander's missingMandatoryOptionValue.
export const MISSING_REQUIRED_OPTION: ReadonlyArray<ReadonlyArray<string>> = [
  ["schedule", "add", EXAMPLE_FLEET_ID],
];

// Commands whose required input is a FLAG the handler validates, not a
// commander declaration. They already spoke the house error shape before
// M171; they are swept alongside the commander rows so both dialects are
// proven to have converged on one shape and one exit code.
export const HANDLER_VALIDATED_REQUIRED_FLAG: ReadonlyArray<ReadonlyArray<string>> = [
  ["logs"],
  ["grant", "list"],
  ["memory", "list"],
  ["install"],
];

// A syntactically valid invocation whose identifier is malformed. The guard
// runs client-side, so the rejection carries the same shape with no request.
export const MALFORMED_ID_INVOCATIONS: ReadonlyArray<ReadonlyArray<string>> = [
  ["logs", "--fleet", "not-a-uuid"],
  ["stop", "not-a-uuid"],
  ["kill", "not-a-uuid"],
];


export const INVALID_ID_SAMPLES: ReadonlyArray<string> = [
  "not-a-uuid",
  "foo",
  "abc def",
];

export const AUTH_REQUIRED_REPRESENTATIVE: ReadonlyArray<ReadonlyArray<string>> = [
  ["doctor"],
  ["workspace", "list"],
  ["api-key", "list"],
  ["connector", "list"],
  ["billing", "show"],
  ["list"],
  // The catalogue is bearer-authed (handlers/model_library.zig), so `models`
  // must fail on the auth guard rather than dialing and surfacing a transport
  // error — the read happens before there is anything to read with.
  ["models"],
];
