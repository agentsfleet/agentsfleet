// Fleet subtree of the agentsfleet command program. Pure construction;
// caller (cli-tree.ts#buildProgram) passes the parent program, the
// already-wired handler map, and the shared mutable `state` object that
// runHandler writes exit codes onto. Kept in its own file so the
// LENGTH GATE on cli-tree.ts does not block future fleet verbs.
//
// Shape mirrors the sibling build*Tree helpers in cli-tree.ts — top-level
// imperative verbs (install / list / status / stop / resume / kill /
// delete / logs / events / steer) plus the `fleet` group for
// update-in-place verbs and the `credential` group for the vault.

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
    .command("templates")
    .description("Browse the first-party Fleet template gallery")
    .action(actionFor("fleet.templates", (frame) => runHandler(state, frame, handlers.fleet.templates)));

  program
    .command("install")
    .description("Register a Fleet from a template (--template) or local skill bundle (--from)")
    // Path existence is validated by loadSkillFromPath inside the handler
    // so the failure path emits ERR_PATH_NOT_FOUND with the friendly
    // remap message instead of commander's generic "path does not exist".
    .option(FLAG_FROM_PATH, SKILL_BUNDLE_PATH, parsePathOption({ mustExist: false }))
    .option(FLAG_TEMPLATE_ID, TEMPLATE_ID_DESC, parseStringOption)
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
      "  agentsfleet templates | install | stop | resume | kill | delete",
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
    .option(FLAG_CURSOR_TOKEN, NEXT_CURSOR_FROM_A_PREVIOUS_PAGE)
    .option(FLAG_LIMIT_N, PAGE_SIZE, parseIntOption(LIST_LIMIT_BOUNDS))
    .action(actionFor("fleet.list", (frame) => runHandler(state, frame, handlers.fleet.list)));

  program
    .command("status [fleet_id]")
    .description("Show fleet status (workspace-wide if no id)")
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

  const credential = program
    .command("credential")
    .description("Workspace credential vault");

  // Two ways to supply the body: the generic `--data <json>` blob, or the
  // typed custom-endpoint flags (`--provider openai-compatible --base-url
  // <url> --api-key <key> [--model <m>]`) that compose the same JSON object.
  // `--base-url` runs parseHttpsUrlOption at PARSE time, so a non-https URL
  // exits non-zero with NO network call (full SSRF check stays server-side).
  credential.command("add <name>")
    .description("Store a credential JSON object")
    .option("--data <json>", "Credential JSON object, or @- to read stdin")
    .option(FLAG_PROVIDER, `Provider id (use '${OPENAI_COMPATIBLE_PROVIDER}' for a custom endpoint)`, parseStringOption)
    .option(FLAG_BASE_URL, "Custom endpoint base URL (https; required for a custom-endpoint provider)", parseHttpsUrlOption)
    .option(FLAG_API_KEY, "Provider API key for the typed custom-endpoint form")
    .option(FLAG_MODEL_OPT, "Default model identifier for the typed custom-endpoint form", parseStringOption)
    .option("--force", "Overwrite if a credential with this name already exists")
    .action(actionFor("fleet.credential.add", (frame) => runHandler(state, frame, handlers.fleet.credential.add)));

  credential.command("show <name>")
    .description("Confirm a credential exists (never echoes secret bytes)")
    .action(actionFor("fleet.credential.show", (frame) => runHandler(state, frame, handlers.fleet.credential.show)));

  credential.command(COMMAND_LIST)
    .description("List credentials in the workspace vault")
    .action(actionFor("fleet.credential.list", (frame) => runHandler(state, frame, handlers.fleet.credential.list)));

  credential.command("delete <name>")
    .description("Delete a credential from the workspace vault")
    .action(actionFor("fleet.credential.delete", (frame) => runHandler(state, frame, handlers.fleet.credential.delete)));
}
const FLAG_CURSOR_TOKEN = "--cursor <token>" as const;
const FLAG_FROM_PATH = "--from <path>" as const;
const FLAG_TEMPLATE_ID = "--template <id>" as const;
const FLAG_NAME = "--name <name>" as const;
const TEMPLATE_ID_DESC = "Template id from `agentsfleet templates`" as const;
const NAME_DESC =
  "Override the fleet name (install the same bundle more than once)" as const;
const FLAG_LIMIT_N = "--limit <n>" as const;
const PAGE_SIZE = "Page size" as const;
const SKILL_BUNDLE_PATH = "Skill bundle path" as const;
const FLAG_PROVIDER = "--provider <id>" as const;
const FLAG_BASE_URL = "--base-url <url>" as const;
const FLAG_API_KEY = "--api-key <key>" as const;
const FLAG_MODEL_OPT = "--model <name>" as const;
const COMMAND_LIST = "list" as const;
const NEXT_CURSOR_FROM_A_PREVIOUS_PAGE = "next_cursor from a previous page" as const;
