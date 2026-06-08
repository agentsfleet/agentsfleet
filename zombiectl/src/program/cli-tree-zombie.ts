// Zombie subtree of the zombiectl command program. Pure construction;
// caller (cli-tree.ts#buildProgram) passes the parent program, the
// already-wired handler map, and the shared mutable `state` object that
// runHandler writes exit codes onto. Kept in its own file so the
// LENGTH GATE on cli-tree.ts does not block future zombie verbs.
//
// Shape mirrors the sibling build*Tree helpers in cli-tree.ts — top-level
// imperative verbs (install / list / status / stop / resume / kill /
// delete / logs / events / steer) plus the `zombie` group for
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

export function buildZombieTree(
  program: Command,
  handlers: Handlers,
  state: ProgramState,
  { actionFor, runHandler }: ActionDispatch,
): void {
  program
    .command("install")
    .description("Register a zombie from a local skill bundle")
    // Path existence is validated by loadSkillFromPath inside the handler
    // so the failure path emits ERR_PATH_NOT_FOUND with the friendly
    // remap message instead of commander's generic "path does not exist".
    .option(FLAG_FROM_PATH, SKILL_BUNDLE_PATH, parsePathOption({ mustExist: false }))
    .action(actionFor("zombie.install", (frame) => runHandler(state, frame, handlers.zombie.install)));

  const zombieGroup = program
    .command("zombie")
    .description("Zombie management subcommands");

  zombieGroup
    .command("update <zombie_id>")
    .description("Re-parse and PATCH a zombie's TRIGGER.md + SKILL.md from a local bundle")
    .option(FLAG_FROM_PATH, SKILL_BUNDLE_PATH, parsePathOption({ mustExist: false }))
    .action(actionFor("zombie.update", (frame) => runHandler(state, frame, handlers.zombie.update)));

  program
    .command(COMMAND_LIST)
    .description("List zombies in the active workspace (paginated)")
    .option("--workspace-id <id>", "Workspace ID override", parseIdOption)
    .option(FLAG_CURSOR_TOKEN, NEXT_CURSOR_FROM_A_PREVIOUS_PAGE)
    .option(FLAG_LIMIT_N, PAGE_SIZE, parseIntOption(LIST_LIMIT_BOUNDS))
    .action(actionFor("zombie.list", (frame) => runHandler(state, frame, handlers.zombie.list)));

  program
    .command("status [zombie_id]")
    .description("Show zombie status (workspace-wide if no id)")
    .action(actionFor("zombie.status", (frame) => runHandler(state, frame, handlers.zombie.status)));

  program
    .command("stop <zombie_id>")
    .description("Halt the running session (resumable)")
    .action(actionFor("zombie.stop", (frame) => runHandler(state, frame, handlers.zombie.stop)));

  program
    .command("resume <zombie_id>")
    .description("Resume from stopped or auto-paused")
    .action(actionFor("zombie.resume", (frame) => runHandler(state, frame, handlers.zombie.resume)));

  program
    .command("kill <zombie_id>")
    .description("Mark terminal (irreversible)")
    .action(actionFor("zombie.kill", (frame) => runHandler(state, frame, handlers.zombie.kill)));

  program
    .command("delete <zombie_id>")
    .description("Hard-delete a killed zombie")
    .action(actionFor("zombie.delete", (frame) => runHandler(state, frame, handlers.zombie.delete)));

  program
    .command("logs [zombie_id]")
    .description("Tail zombie activity")
    .option("--zombie <id>", "Zombie ID (alternative to positional)", parseIdOption)
    .option(FLAG_LIMIT_N, "Number of events to show", parseIntOption(EVENTS_LIMIT_BOUNDS))
    .option(FLAG_CURSOR_TOKEN, NEXT_CURSOR_FROM_A_PREVIOUS_PAGE)
    .action(actionFor("zombie.logs", (frame) => runHandler(state, frame, handlers.zombie.logs)));

  program
    .command("events <zombie_id>")
    .description("Page through historical events")
    .option("--actor <glob>", "Filter by actor glob")
    .option("--since <when>", "RFC 3339 or duration (e.g. 2h)")
    .option(FLAG_CURSOR_TOKEN, NEXT_CURSOR_FROM_A_PREVIOUS_PAGE)
    .option(FLAG_LIMIT_N, PAGE_SIZE, parseIntOption(EVENTS_LIMIT_BOUNDS))
    .action(actionFor("zombie.events", (frame) => runHandler(state, frame, handlers.zombie.events)));

  program
    .command("steer <zombie_id> [message]")
    .description("Send a message; stream the response")
    .action(actionFor("zombie.steer", (frame) => runHandler(state, frame, handlers.zombie.steer)));

  const credential = program
    .command("credential")
    .description("Workspace credential vault");

  credential.command("add <name>")
    .description("Store a credential JSON object")
    .option("--data <json>", "Credential JSON object, or @- to read stdin")
    .option("--force", "Overwrite if a credential with this name already exists")
    .action(actionFor("zombie.credential.add", (frame) => runHandler(state, frame, handlers.zombie.credential.add)));

  credential.command("show <name>")
    .description("Confirm a credential exists (never echoes secret bytes)")
    .action(actionFor("zombie.credential.show", (frame) => runHandler(state, frame, handlers.zombie.credential.show)));

  credential.command(COMMAND_LIST)
    .description("List credentials in the workspace vault")
    .action(actionFor("zombie.credential.list", (frame) => runHandler(state, frame, handlers.zombie.credential.list)));

  credential.command("delete <name>")
    .description("Delete a credential from the workspace vault")
    .action(actionFor("zombie.credential.delete", (frame) => runHandler(state, frame, handlers.zombie.credential.delete)));
}
const FLAG_CURSOR_TOKEN = "--cursor <token>" as const;
const FLAG_FROM_PATH = "--from <path>" as const;
const FLAG_LIMIT_N = "--limit <n>" as const;
const PAGE_SIZE = "Page size" as const;
const SKILL_BUNDLE_PATH = "Skill bundle path" as const;
const COMMAND_LIST = "list" as const;
const NEXT_CURSOR_FROM_A_PREVIOUS_PAGE = "next_cursor from a previous page" as const;
