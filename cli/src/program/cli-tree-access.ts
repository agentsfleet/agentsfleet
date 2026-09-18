import type { Command } from "commander";
import {
  parseEnumOption,
  parseIdOption,
} from "./validators.ts";
import type {
  ActionDispatch,
  Handlers,
  ProgramState,
} from "./cli-tree-types.ts";
import { API_KEY_SORTS } from "../constants/api-key.ts";

const FLAG_WORKSPACE_ID = "--workspace <id>" as const;
const WORKSPACE_ID = "Workspace ID" as const;
const COMMAND_LIST = "list" as const;
const API_KEY_ID_ARG = "<api_key_id>" as const;
const API_KEY_ID_HELP = "API key ID" as const;
const GATE_ID_ARG = "<gate_id>" as const;
const GATE_ID_HELP = "Approval gate ID" as const;
const FLAG_FLEET_ID = "--fleet <id>" as const;
const FLEET_ID_HELP = "Show gates for this Fleet only" as const;

export function buildAccessTree(
  program: Command,
  handlers: Handlers,
  state: ProgramState,
  dispatch: ActionDispatch,
): void {
  buildApiKeyTree(program, handlers, state, dispatch);
  buildConnectorTree(program, handlers, state, dispatch);
  buildApprovalsTree(program, handlers, state, dispatch);
}

// Approve and deny are separate subcommands rather than one `--decision` flag:
// the daemon gives each its own path segment and its own capability, and a
// decision reachable by a flag default is a decision nobody made.
function buildApprovalsTree(
  program: Command,
  handlers: Handlers,
  state: ProgramState,
  { actionFor, runHandler }: ActionDispatch,
): void {
  const approvals = program
    .command("approvals")
    .description("Review and decide pending approval gates");

  approvals.command(COMMAND_LIST)
    .description("List approval gates in the active workspace")
    .option(FLAG_FLEET_ID, FLEET_ID_HELP, parseIdOption)
    .action(actionFor("approvals.list", (frame) => runHandler(state, frame, handlers.approvals.list)));

  approvals.command("show")
    .description("Show one gate's proposed action and blast radius")
    .argument(GATE_ID_ARG, GATE_ID_HELP, parseIdOption)
    .action(actionFor("approvals.show", (frame) => runHandler(state, frame, handlers.approvals.show)));

  approvals.command("approve")
    .description("Approve a gate and let the Fleet continue")
    .argument(GATE_ID_ARG, GATE_ID_HELP, parseIdOption)
    .action(actionFor("approvals.approve", (frame) => runHandler(state, frame, handlers.approvals.approve)));

  approvals.command("deny")
    .description("Deny a gate and stop the proposed action")
    .argument(GATE_ID_ARG, GATE_ID_HELP, parseIdOption)
    .action(actionFor("approvals.deny", (frame) => runHandler(state, frame, handlers.approvals.deny)));
}

function buildApiKeyTree(
  program: Command,
  handlers: Handlers,
  state: ProgramState,
  { actionFor, runHandler }: ActionDispatch,
): void {
  const apiKey = program
    .command("api-key")
    .description("Manage tenant API keys");

  apiKey.command("create")
    .description("Create a tenant API key")
    .option("--name <name>", "Human-readable key name")
    .option("--description <desc>", "Optional description")
    .action(actionFor("api-key.create", (frame) => runHandler(state, frame, handlers.apiKey.create)));

  apiKey.command(COMMAND_LIST)
    .description("List tenant API keys")
    .option("--sort <field>", "Sort order", parseEnumOption(API_KEY_SORTS))
    .action(actionFor("api-key.list", (frame) => runHandler(state, frame, handlers.apiKey.list)));

  apiKey.command("revoke")
    .description("Revoke a tenant API key")
    .argument(API_KEY_ID_ARG, API_KEY_ID_HELP, parseIdOption)
    .action(actionFor("api-key.revoke", (frame) => runHandler(state, frame, handlers.apiKey.revoke)));

  apiKey.command("delete")
    .description("Delete a revoked tenant API key")
    .argument(API_KEY_ID_ARG, API_KEY_ID_HELP, parseIdOption)
    .action(actionFor("api-key.delete", (frame) => runHandler(state, frame, handlers.apiKey.delete)));
}

function buildConnectorTree(
  program: Command,
  handlers: Handlers,
  state: ProgramState,
  { actionFor, runHandler }: ActionDispatch,
): void {
  const connector = program
    .command("connector")
    .description("Inspect workspace connectors");

  connector.command(COMMAND_LIST)
    .description("List connector setup and connection state")
    .option(FLAG_WORKSPACE_ID, WORKSPACE_ID, parseIdOption)
    .action(actionFor("connector.list", (frame) => runHandler(state, frame, handlers.connector.list)));

  connector.command("status <provider>")
    .description("Show connector state")
    .option(FLAG_WORKSPACE_ID, WORKSPACE_ID, parseIdOption)
    .action(actionFor("connector.status", (frame) => runHandler(state, frame, handlers.connector.status)));
}
