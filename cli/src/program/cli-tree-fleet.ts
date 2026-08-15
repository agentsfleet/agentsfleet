// Fleet subtree of the agentsfleet command program. Pure construction;
// caller (cli-tree.ts#buildProgram) passes the parent program, the
// already-wired handler map, and the shared mutable `state` object that
// runHandler writes exit codes onto. Kept in its own file so the
// LENGTH GATE on cli-tree.ts does not block future fleet verbs.
//
// Shape mirrors the sibling build*Tree helpers in cli-tree.ts — top-level
// imperative verbs (install / list / status / stop / resume / kill /
// delete / logs / events / steer) plus the `fleet` group for
// update-in-place verbs and the `secret` group for the vault.

import type { Command } from "commander";
import {
  parseIntOption,
  parseIdOption,
  parsePathOption,
  parseStringOption,
  parseHttpsUrlOption,
} from "./validators.ts";
import { OPENAI_COMPATIBLE_PROVIDER } from "../constants/custom-endpoint.ts";
import type {
  ActionDispatch,
  Handlers,
  ProgramState,
} from "./cli-tree-types.ts";

const LIST_LIMIT_BOUNDS = { min: 1, max: 200 };
const EVENTS_LIMIT_BOUNDS = { min: 1, max: 500 };

export function buildFleetTree(
  program: Command,
  handlers: Handlers,
  state: ProgramState,
  { actionFor, runHandler }: ActionDispatch,
): void {
  program
    .command("library")
    .description("Browse the first-party Fleet library gallery")
    .action(actionFor("fleet.library", (frame) => runHandler(state, frame, handlers.fleet.library)));

  // The CLI peer of the dashboard's model picker. Both read GET /v1/models, so
  // `--provider` and `--model` have a discoverable source instead of being two
  // identifiers the operator has to already know.
  program
    .command("models")
    .description("List the model catalogue this server serves")
    .option(FLAG_PROVIDER, "Show only this provider's models", parseStringOption)
    .action(actionFor("fleet.models", (frame) => runHandler(state, frame, handlers.fleet.models)));

  program
    .command("install")
    .description("Install a Fleet from an onboarded library (--library <id>)")
    .option(FLAG_LIBRARY_ID, LIBRARY_ID_DESC, parseStringOption)
    .option(FLAG_NAME, NAME_DESC, parseStringOption)
    .action(actionFor("fleet.install", (frame) => runHandler(state, frame, handlers.fleet.install)));

  const fleetGroup = program
    .command("fleet")
    .description("Fleet management subcommands");

  // The lifecycle verbs live at the top level, not under `fleet` — so
  // `agentsfleet fleet list` would resolve to this group's help and show
  // only `update`, with no hint that `list` is one level up. Spell the
  // split out here; the top-level `--help` command list shows them too.
  fleetGroup.addHelpText(
    "after",
    [
      "",
      "Fleet lifecycle verbs are top-level commands, not under `fleet`:",
      "  agentsfleet list | status | logs | events | steer",
      "  agentsfleet library | install | stop | resume | kill | delete",
      "This group holds in-place updates only. Run `agentsfleet --help`",
      "for the full command list.",
    ].join("\n"),
  );

  fleetGroup
    .command("update <fleet_id>")
    .description("Re-parse and PATCH a Fleet's TRIGGER.md + SKILL.md from a local bundle")
    .option(FLAG_FROM_PATH, SKILL_BUNDLE_PATH, parsePathOption({ mustExist: false }))
    .action(actionFor("fleet.update", (frame) => runHandler(state, frame, handlers.fleet.update)));

  program
    .command(COMMAND_LIST)
    .description("List fleets in the active workspace (paginated)")
    .option("--workspace-id <id>", "Workspace ID override", parseIdOption)
    .option(FLAG_STARTING_AFTER, NEXT_CURSOR_FROM_A_PREVIOUS_PAGE)
    .option(FLAG_LIMIT_N, PAGE_SIZE, parseIntOption(LIST_LIMIT_BOUNDS))
    .action(actionFor("fleet.list", (frame) => runHandler(state, frame, handlers.fleet.list)));

  program
    .command("status")
    .description("Show status for every fleet in the active workspace")
    .action(actionFor("fleet.status", (frame) => runHandler(state, frame, handlers.fleet.status)));

  program
    .command("stop <fleet_id>")
    .description("Halt the running session (resumable)")
    .action(actionFor("fleet.stop", (frame) => runHandler(state, frame, handlers.fleet.stop)));

  program
    .command("resume <fleet_id>")
    .description("Resume from stopped or auto-paused")
    .action(actionFor("fleet.resume", (frame) => runHandler(state, frame, handlers.fleet.resume)));

  program
    .command("kill <fleet_id>")
    .description("Mark terminal (irreversible)")
    .action(actionFor("fleet.kill", (frame) => runHandler(state, frame, handlers.fleet.kill)));

  program
    .command("delete <fleet_id>")
    .description("Hard-delete a killed fleet")
    .action(actionFor("fleet.delete", (frame) => runHandler(state, frame, handlers.fleet.delete)));

  program
    .command("logs [fleet_id]")
    .description("Tail fleet activity")
    .option("--fleet <id>", "Fleet ID (alternative to positional)", parseIdOption)
    .option(FLAG_LIMIT_N, "Number of events to show", parseIntOption(EVENTS_LIMIT_BOUNDS))
    .option(FLAG_CURSOR_TOKEN, NEXT_CURSOR_FROM_A_PREVIOUS_PAGE)
    .action(actionFor("fleet.logs", (frame) => runHandler(state, frame, handlers.fleet.logs)));

  program
    .command("events <fleet_id>")
    .description("Page through historical events")
    .option("--actor <glob>", "Filter by actor glob")
    .option("--since <when>", "RFC 3339 or duration (e.g. 2h)")
    .option(FLAG_CURSOR_TOKEN, NEXT_CURSOR_FROM_A_PREVIOUS_PAGE)
    .option(FLAG_LIMIT_N, PAGE_SIZE, parseIntOption(EVENTS_LIMIT_BOUNDS))
    .action(actionFor("fleet.events", (frame) => runHandler(state, frame, handlers.fleet.events)));

  program
    .command("steer <fleet_id> [message]")
    .description("Send a message; stream the response")
    .action(actionFor("fleet.steer", (frame) => runHandler(state, frame, handlers.fleet.steer)));

  const secret = program
    .command("secret")
    .description("Workspace secret vault");

  // Two ways to supply the body: the generic `--data <json>` blob, or the
  // typed provider flags in one of two shapes — a named provider
  // (`--provider <id> --api-key <key> --model <m>`) or a custom endpoint
  // (`--provider openai-compatible --base-url <url> --model <m>
  // [--api-key <key>]`) — composing the same JSON object.
  // `--base-url` runs parseHttpsUrlOption at PARSE time, so a non-https URL
  // exits non-zero with NO network call (full SSRF check stays server-side).
  // `--provider` cannot be checked here: its accepted set is whatever this
  // server's catalogue serves, so the handler validates it against
  // `GET /v1/models`. `--data` stays unconstrained (generic blob).
  secret.command("create <name>")
    .description("Store a secret JSON object")
    .option(FLAG_DATA_JSON, "Secret JSON object, or @- to read stdin")
    .option(FLAG_PROVIDER, DESC_PROVIDER, parseStringOption)
    .option(FLAG_BASE_URL, DESC_BASE_URL, parseHttpsUrlOption)
    .option(FLAG_API_KEY, DESC_API_KEY)
    .option(FLAG_MODEL_OPT, DESC_MODEL_OPT, parseStringOption)
    .action(actionFor("fleet.secret.create", (frame) => runHandler(state, frame, handlers.fleet.secret.create)));

  // Replaces the stored body in place: the name stays claimed for the whole
  // call, so fleets that require it keep resolving. `delete` + `create` also
  // replaces a value, but leaves a window where the name does not exist.
  // Replacement is total — a field absent from the new body is absent after.
  secret.command("update <name>")
    .description("Replace a secret's stored body without releasing the name")
    .option(FLAG_DATA_JSON, "Replacement JSON object, or @- to read stdin")
    .option(FLAG_PROVIDER, DESC_PROVIDER, parseStringOption)
    .option(FLAG_BASE_URL, DESC_BASE_URL, parseHttpsUrlOption)
    .option(FLAG_API_KEY, DESC_API_KEY)
    .option(FLAG_MODEL_OPT, DESC_MODEL_OPT, parseStringOption)
    .action(actionFor("fleet.secret.update", (frame) => runHandler(state, frame, handlers.fleet.secret.update)));

  secret.command("show <name>")
    .description("Confirm a secret exists (never echoes secret bytes)")
    .action(actionFor("fleet.secret.show", (frame) => runHandler(state, frame, handlers.fleet.secret.show)));

  secret.command(COMMAND_LIST)
    .description("List secrets in the workspace vault")
    .action(actionFor("fleet.secret.list", (frame) => runHandler(state, frame, handlers.fleet.secret.list)));

  secret.command("delete <name>")
    .description("Delete a secret from the workspace vault")
    .action(actionFor("fleet.secret.delete", (frame) => runHandler(state, frame, handlers.fleet.secret.delete)));
}
// `fleet list` speaks the guideline spelling; `fleet logs` / `fleet events`
// keep --cursor until their endpoints rename (scoped follow-up in the spec).
const FLAG_STARTING_AFTER = "--starting-after <id>" as const;
const FLAG_CURSOR_TOKEN = "--cursor <token>" as const;
const FLAG_FROM_PATH = "--from <path>" as const;
const FLAG_LIBRARY_ID = "--library <id>" as const;
const FLAG_NAME = "--name <name>" as const;
const LIBRARY_ID_DESC = "Library id from `agentsfleet library`" as const;
const NAME_DESC =
  "Override the fleet name (install the same bundle more than once)" as const;
const FLAG_LIMIT_N = "--limit <n>" as const;
const PAGE_SIZE = "Page size" as const;
const SKILL_BUNDLE_PATH = "Skill bundle path" as const;
const FLAG_DATA_JSON = "--data <json>" as const;
// No enum parser here, and that is the design. The accepted set is whatever
// `GET /v1/models` serves on the server the caller is pointed at, so it cannot
// be known at parse time and differs between environments. The check runs in
// the handler against the live catalogue (lib/model-catalogue.ts) — the same
// bytes the dashboard's provider dropdown is built from.
const DESC_PROVIDER =
  `Provider id from \`agentsfleet models\` (use '${OPENAI_COMPATIBLE_PROVIDER}' with --base-url for an endpoint the catalogue does not carry)`;
const DESC_BASE_URL = "Custom endpoint base URL (https; required for a custom-endpoint provider)" as const;
const DESC_API_KEY = "Provider API key (required with a named --provider, optional for a keyless custom endpoint)" as const;
const DESC_MODEL_OPT = "Default model identifier (required with --provider)" as const;
const FLAG_PROVIDER = "--provider <id>" as const;
const FLAG_BASE_URL = "--base-url <url>" as const;
const FLAG_API_KEY = "--api-key <key>" as const;
const FLAG_MODEL_OPT = "--model <name>" as const;
const COMMAND_LIST = "list" as const;
const NEXT_CURSOR_FROM_A_PREVIOUS_PAGE = "next_cursor from a previous page" as const;
