import type { Command } from "commander";
import {
  parseEnumOption,
  parseIdOption,
  parseIntOption,
} from "./validators.ts";
import type {
  ActionDispatch,
  Handlers,
  ProgramState,
} from "./cli-tree-types.ts";
import {
  API_KEY_SORTS,
  MAX_API_KEY_PAGE_SIZE,
} from "../constants/api-key.ts";

const PAGE_BOUNDS = { min: 1 };
const PAGE_SIZE_BOUNDS = { min: 1, max: MAX_API_KEY_PAGE_SIZE };
const FLAG_WORKSPACE_ID = "--workspace <id>" as const;
const WORKSPACE_ID = "Workspace ID" as const;
const COMMAND_LIST = "list" as const;
const API_KEY_ID_ARG = "<api_key_id>" as const;
const API_KEY_ID_HELP = "API key ID" as const;

export function buildAccessTree(
  program: Command,
  handlers: Handlers,
  state: ProgramState,
  dispatch: ActionDispatch,
): void {
  buildApiKeyTree(program, handlers, state, dispatch);
  buildConnectorTree(program, handlers, state, dispatch);
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
    .option("--page <n>", "Page number", parseIntOption(PAGE_BOUNDS))
    .option("--page-size <n>", "Page size", parseIntOption(PAGE_SIZE_BOUNDS))
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
