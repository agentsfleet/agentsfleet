// Schedule command subtree. Pure commander construction; handlers live in
// commands/fleet_schedule.ts and are bound in handlers-bind-schedule.ts.

import type { Command } from "commander";
import { parseStringOption } from "./validators.ts";
import type {
  ActionDispatch,
  Handlers,
  ProgramState,
} from "./cli-tree-types.ts";

export function buildScheduleTree(
  program: Command,
  handlers: Handlers,
  state: ProgramState,
  { actionFor, runHandler }: ActionDispatch,
): void {
  const schedule = program
    .command("schedule")
    .description("Manage hosted Fleet schedules");

  schedule
    .command("add <fleet_id>")
    .description("Create a hosted schedule for a Fleet")
    .requiredOption(FLAG_CRON, DESC_CRON, parseStringOption)
    .requiredOption(FLAG_MESSAGE, DESC_MESSAGE, parseStringOption)
    .option(FLAG_TIMEZONE, DESC_TIMEZONE_DEFAULT, parseStringOption)
    .option(FLAG_WORKSPACE, WORKSPACE_DESC, parseStringOption)
    .action(actionFor("schedule.add", (frame) => runHandler(state, frame, handlers.schedule.add)));

  schedule
    .command("list <fleet_id>")
    .description("List hosted schedules for a Fleet")
    .option(FLAG_WORKSPACE, WORKSPACE_DESC, parseStringOption)
    .action(actionFor("schedule.list", (frame) => runHandler(state, frame, handlers.schedule.list)));

  schedule
    .command("update <fleet_id> <schedule_id>")
    .description("Update a hosted schedule")
    .option(FLAG_CRON, DESC_CRON, parseStringOption)
    .option(FLAG_MESSAGE, DESC_MESSAGE, parseStringOption)
    .option(FLAG_TIMEZONE, DESC_TIMEZONE, parseStringOption)
    .option("--status <status>", "active or paused", parseStringOption)
    .option(FLAG_WORKSPACE, WORKSPACE_DESC, parseStringOption)
    .action(actionFor("schedule.update", (frame) => runHandler(state, frame, handlers.schedule.update)));

  schedule
    .command("rm <fleet_id> <schedule_id>")
    .description("Remove a hosted schedule")
    .option(FLAG_WORKSPACE, WORKSPACE_DESC, parseStringOption)
    .action(actionFor("schedule.rm", (frame) => runHandler(state, frame, handlers.schedule.rm)));

  schedule
    .command("status <fleet_id> <schedule_id>")
    .description("Show one hosted schedule")
    .option(FLAG_WORKSPACE, WORKSPACE_DESC, parseStringOption)
    .action(actionFor("schedule.status", (frame) => runHandler(state, frame, handlers.schedule.status)));

  schedule
    .command("sync <fleet_id> <schedule_id>")
    .description("Re-apply a hosted schedule to QStash")
    .option(FLAG_WORKSPACE, WORKSPACE_DESC, parseStringOption)
    .action(actionFor("schedule.sync", (frame) => runHandler(state, frame, handlers.schedule.sync)));
}

const FLAG_WORKSPACE = "--workspace <id>" as const;
const FLAG_CRON = "--cron <expr>" as const;
const FLAG_MESSAGE = "--message <text>" as const;
const FLAG_TIMEZONE = "--timezone <tz>" as const;
const WORKSPACE_DESC = "Workspace ID override" as const;
const DESC_CRON = "Cron expression" as const;
const DESC_MESSAGE = "Message sent to the Fleet" as const;
const DESC_TIMEZONE = "IANA timezone" as const;
const DESC_TIMEZONE_DEFAULT = "IANA timezone (default: UTC)" as const;
