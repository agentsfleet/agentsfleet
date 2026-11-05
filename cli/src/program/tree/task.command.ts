// Task commands remain children of fleet management and use its existing workspace selection.

import { Command } from "effect/cli";
import { guardedHandler } from "./guarded-handler.ts";
import { fleetIdArgument, goalRevisionArgument, taskIdArgument } from "./flags.ts";
import { taskChangeEffect, taskListEffect, taskShowEffect } from "../../commands/fleet_tasks.ts";

const target = {fleetId: fleetIdArgument, taskId: taskIdArgument};
const list = Command.make("list", {fleetId: fleetIdArgument}).pipe(
  Command.withDescription("List unfinished work and confirmed native states"),
  guardedHandler(({fleetId}) => taskListEffect(fleetId)),
);
const show = Command.make("show", target).pipe(
  Command.withDescription("Read an objective, saved success criteria and cumulative progress"),
  guardedHandler(({fleetId, taskId}) => taskShowEffect(fleetId, taskId)),
);
const pause = Command.make("pause", target).pipe(
  Command.withDescription("Pause new task work while retaining progress"),
  guardedHandler(({fleetId, taskId}) => taskChangeEffect(fleetId, taskId, {kind: "control", state: "paused"})),
);
const resume = Command.make("resume", target).pipe(
  Command.withDescription("Resume paused or blocked work within its remaining limits"),
  guardedHandler(({fleetId, taskId}) => taskChangeEffect(fleetId, taskId, {kind: "control", state: "ready"})),
);
const cancel = Command.make("cancel", target).pipe(
  Command.withDescription("Stop new work permanently; published changes remain"),
  guardedHandler(({fleetId, taskId}) => taskChangeEffect(fleetId, taskId, {kind: "control", state: "cancelled"})),
);
const adopt = Command.make("adopt", {...target, revisionId: goalRevisionArgument}).pipe(
  Command.withDescription("Explicitly adopt reviewed success criteria without resetting progress or widening access"),
  guardedHandler(({fleetId, taskId, revisionId}) => taskChangeEffect(fleetId, taskId, {kind: "adopt", revisionId})),
);

export const taskCommand = Command.make("tasks").pipe(
  Command.withDescription("Inspect and control durable fleet work"),
  Command.withSubcommands([list, show, pause, resume, cancel, adopt]),
);
