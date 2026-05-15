// Zombie subtree of the zombiectl command program. Pure construction;
// caller (cli-tree.js#buildProgram) passes the parent program, the
// already-wired handler map, and the shared mutable `state` object that
// runHandler writes exit codes onto. Kept in its own file so the
// LENGTH GATE on cli-tree.js does not block future zombie verbs.
//
// Shape mirrors the sibling build*Tree helpers in cli-tree.js — top-level
// imperative verbs (install / list / status / stop / resume / kill /
// delete / logs / events / steer) plus the `zombie` group for
// update-in-place verbs and the `credential` group for the vault.

import {
  parseIntOption,
  parseIdOption,
  parsePathOption,
} from "./validators.js";

const LIST_LIMIT_BOUNDS = { min: 1, max: 200 };
const EVENTS_LIMIT_BOUNDS = { min: 1, max: 500 };

export function buildZombieTree(program, handlers, state, { actionFor, runHandler }) {
  program
    .command("install")
    .description("Register a zombie from a local skill bundle")
    // Path existence is validated by loadSkillFromPath inside the handler
    // so the failure path emits ERR_PATH_NOT_FOUND with the friendly
    // remap message instead of commander's generic "path does not exist".
    .option("--from <path>", "Skill bundle path", parsePathOption({ mustExist: false }))
    .action(actionFor("zombie.install", (frame) => runHandler(state, frame, handlers.zombie.install)));

  const zombieGroup = program
    .command("zombie")
    .description("Zombie management subcommands");

  zombieGroup
    .command("update <zombie_id>")
    .description("Re-parse and PATCH a zombie's TRIGGER.md + SKILL.md from a local bundle")
    .option("--from <path>", "Skill bundle path", parsePathOption({ mustExist: false }))
    .action(actionFor("zombie.update", (frame) => runHandler(state, frame, handlers.zombie.update)));

  program
    .command("list")
    .description("List zombies in the active workspace (paginated)")
    .option("--workspace-id <id>", "Workspace ID override", parseIdOption)
    .option("--cursor <token>", "next_cursor from a previous page")
    .option("--limit <n>", "Page size", parseIntOption(LIST_LIMIT_BOUNDS))
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
    .option("--limit <n>", "Number of events to show", parseIntOption(EVENTS_LIMIT_BOUNDS))
    .option("--cursor <token>", "next_cursor from a previous page")
    .action(actionFor("zombie.logs", (frame) => runHandler(state, frame, handlers.zombie.logs)));

  program
    .command("events <zombie_id>")
    .description("Page through historical events")
    .option("--actor <glob>", "Filter by actor glob")
    .option("--since <when>", "RFC 3339 or duration (e.g. 2h)")
    .option("--cursor <token>", "next_cursor from a previous page")
    .option("--limit <n>", "Page size", parseIntOption(EVENTS_LIMIT_BOUNDS))
    .action(actionFor("zombie.events", (frame) => runHandler(state, frame, handlers.zombie.events)));

  program
    .command("steer <zombie_id> <message>")
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

  credential.command("list")
    .description("List credentials in the workspace vault")
    .action(actionFor("zombie.credential.list", (frame) => runHandler(state, frame, handlers.zombie.credential.list)));

  credential.command("delete <name>")
    .description("Delete a credential from the workspace vault")
    .action(actionFor("zombie.credential.delete", (frame) => runHandler(state, frame, handlers.zombie.credential.delete)));
}
