// Agent subtree of the agentsfleet command program. Pure construction;
// caller (cli-tree.ts#buildProgram) passes the parent program, the
// already-wired handler map, and the shared mutable `state` object that
// runHandler writes exit codes onto. Kept in its own file so the
// LENGTH GATE on cli-tree.ts does not block future agent verbs.
//
// Shape mirrors the sibling build*Tree helpers in cli-tree.ts — top-level
// imperative verbs (install / list / status / stop / resume / kill /
// delete / logs / events / steer) plus the `agent` group for
// update-in-place verbs and the `credential` group for the vault.

import type { Command } from "commander";
import {
  parseIntOption,
  parseIdOption,
  parsePathOption,
} from "./validators.ts";
import type {
  ActionDispatch,
  Handlers,
  ProgramState,
} from "./cli-tree-types.ts";

const LIST_LIMIT_BOUNDS = { min: 1, max: 200 };
const EVENTS_LIMIT_BOUNDS = { min: 1, max: 500 };

export function buildAgentTree(
  program: Command,
  handlers: Handlers,
  state: ProgramState,
  { actionFor, runHandler }: ActionDispatch,
): void {
  program
    .command("install")
    .description("Register an agent from a local skill bundle")
    // Path existence is validated by loadSkillFromPath inside the handler
    // so the failure path emits ERR_PATH_NOT_FOUND with the friendly
    // remap message instead of commander's generic "path does not exist".
    .option(FLAG_FROM_PATH, SKILL_BUNDLE_PATH, parsePathOption({ mustExist: false }))
    .action(actionFor("agent.install", (frame) => runHandler(state, frame, handlers.agent.install)));

  const agentGroup = program
    .command("agent")
    .description("Agent management subcommands");

  agentGroup
    .command("update <agent_id>")
    .description("Re-parse and PATCH an agent's TRIGGER.md + SKILL.md from a local bundle")
    .option(FLAG_FROM_PATH, SKILL_BUNDLE_PATH, parsePathOption({ mustExist: false }))
    .action(actionFor("agent.update", (frame) => runHandler(state, frame, handlers.agent.update)));

  program
    .command(COMMAND_LIST)
    .description("List agents in the active workspace (paginated)")
    .option("--workspace-id <id>", "Workspace ID override", parseIdOption)
    .option(FLAG_CURSOR_TOKEN, NEXT_CURSOR_FROM_A_PREVIOUS_PAGE)
    .option(FLAG_LIMIT_N, PAGE_SIZE, parseIntOption(LIST_LIMIT_BOUNDS))
    .action(actionFor("agent.list", (frame) => runHandler(state, frame, handlers.agent.list)));

  program
    .command("status [agent_id]")
    .description("Show agent status (workspace-wide if no id)")
    .action(actionFor("agent.status", (frame) => runHandler(state, frame, handlers.agent.status)));

  program
    .command("stop <agent_id>")
    .description("Halt the running session (resumable)")
    .action(actionFor("agent.stop", (frame) => runHandler(state, frame, handlers.agent.stop)));

  program
    .command("resume <agent_id>")
    .description("Resume from stopped or auto-paused")
    .action(actionFor("agent.resume", (frame) => runHandler(state, frame, handlers.agent.resume)));

  program
    .command("kill <agent_id>")
    .description("Mark terminal (irreversible)")
    .action(actionFor("agent.kill", (frame) => runHandler(state, frame, handlers.agent.kill)));

  program
    .command("delete <agent_id>")
    .description("Hard-delete a killed agent")
    .action(actionFor("agent.delete", (frame) => runHandler(state, frame, handlers.agent.delete)));

  program
    .command("logs [agent_id]")
    .description("Tail agent activity")
    .option("--agent <id>", "Agent ID (alternative to positional)", parseIdOption)
    .option(FLAG_LIMIT_N, "Number of events to show", parseIntOption(EVENTS_LIMIT_BOUNDS))
    .option(FLAG_CURSOR_TOKEN, NEXT_CURSOR_FROM_A_PREVIOUS_PAGE)
    .action(actionFor("agent.logs", (frame) => runHandler(state, frame, handlers.agent.logs)));

  program
    .command("events <agent_id>")
    .description("Page through historical events")
    .option("--actor <glob>", "Filter by actor glob")
    .option("--since <when>", "RFC 3339 or duration (e.g. 2h)")
    .option(FLAG_CURSOR_TOKEN, NEXT_CURSOR_FROM_A_PREVIOUS_PAGE)
    .option(FLAG_LIMIT_N, PAGE_SIZE, parseIntOption(EVENTS_LIMIT_BOUNDS))
    .action(actionFor("agent.events", (frame) => runHandler(state, frame, handlers.agent.events)));

  program
    .command("steer <agent_id> [message]")
    .description("Send a message; stream the response")
    .action(actionFor("agent.steer", (frame) => runHandler(state, frame, handlers.agent.steer)));

  const credential = program
    .command("credential")
    .description("Workspace credential vault");

  credential.command("add <name>")
    .description("Store a credential JSON object")
    .option("--data <json>", "Credential JSON object, or @- to read stdin")
    .option("--force", "Overwrite if a credential with this name already exists")
    .action(actionFor("agent.credential.add", (frame) => runHandler(state, frame, handlers.agent.credential.add)));

  credential.command("show <name>")
    .description("Confirm a credential exists (never echoes secret bytes)")
    .action(actionFor("agent.credential.show", (frame) => runHandler(state, frame, handlers.agent.credential.show)));

  credential.command(COMMAND_LIST)
    .description("List credentials in the workspace vault")
    .action(actionFor("agent.credential.list", (frame) => runHandler(state, frame, handlers.agent.credential.list)));

  credential.command("delete <name>")
    .description("Delete a credential from the workspace vault")
    .action(actionFor("agent.credential.delete", (frame) => runHandler(state, frame, handlers.agent.credential.delete)));
}
const FLAG_CURSOR_TOKEN = "--cursor <token>" as const;
const FLAG_FROM_PATH = "--from <path>" as const;
const FLAG_LIMIT_N = "--limit <n>" as const;
const PAGE_SIZE = "Page size" as const;
const SKILL_BUNDLE_PATH = "Skill bundle path" as const;
const COMMAND_LIST = "list" as const;
const NEXT_CURSOR_FROM_A_PREVIOUS_PAGE = "next_cursor from a previous page" as const;
