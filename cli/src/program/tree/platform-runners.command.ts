import {RunnerAction, RunnerEventType, RunnerHistory} from "../../commands/platform/runner-vocabulary.ts";
import { Option, Schema } from "effect";
import { Command, Flag } from "effect/cli";
import { platformRunnerAction, platformRunnerDelete, platformRunnerEnroll, platformRunnerHistory, platformRunnersList, platformRunnerRotate, platformRunnerShow, platformRunnerUpdate } from "../../commands/platform/runners.ts";
import { guardedHandler } from "./guarded-handler.ts";
import { platformLabelsFlag, platformPagingFlags, platformPolicyFlag, platformTokenFlag } from "./platform-flags.ts";
import { eventTimeFlag, idArgument, requiredIdFlag, requiredTextFlag, sinceFlag } from "./flags.ts";

const opt = Option.getOrUndefined;
const runnerId = {id: idArgument("runner_id", "Platform runner identifier")};
const actionCommand = (name: string, action: RunnerAction) => Command.make(name, runnerId).pipe(
  Command.withDescription(`Request ${name} (runner:write)`), guardedHandler(({id}) => platformRunnerAction(id, action)),
);
const list = Command.make("list", platformPagingFlags).pipe(
  Command.withDescription("List platform runners (runner:read)"),
  guardedHandler(({limit, startingAfter}) => platformRunnersList({limit: opt(limit), startingAfter: opt(startingAfter)})),
);
const show = Command.make("show", runnerId).pipe(
  Command.withDescription("Show policy, capabilities, liveness, counters, and checks (runner:read)"),
  guardedHandler(({id}) => platformRunnerShow(id)),
);
const enroll = Command.make("enroll", {
  host: requiredTextFlag("host-id", "Host identity", "<host>"), policy: platformPolicyFlag,
  labels: platformLabelsFlag, tokenFile: platformTokenFlag,
}).pipe(Command.withDescription("Enroll a trusted host and save its token (runner:enroll)"),
  guardedHandler(({host, policy, labels, tokenFile}) => platformRunnerEnroll(host, {policy, labels: opt(labels)}, tokenFile)));
const update = Command.make("update", {...runnerId, policy: platformPolicyFlag.pipe(Flag.optional), labels: platformLabelsFlag}).pipe(
  Command.withDescription("Replace policy and/or labels (runner:write)"),
  guardedHandler(({id, policy, labels}) => platformRunnerUpdate(id, {policy: opt(policy), labels: opt(labels)})),
);
const rotate = Command.make("rotate-token", {...runnerId, tokenFile: platformTokenFlag}).pipe(
  Command.withDescription("Replace the runner token and save it once (runner:write)"),
  guardedHandler(({id, tokenFile}) => platformRunnerRotate(id, tokenFile)),
);
const remove = Command.make("delete", runnerId).pipe(
  Command.withDescription("Delete a revoked runner and its history (runner:write)"), guardedHandler(({id}) => platformRunnerDelete(id)),
);
const events = Command.make("events", {...runnerId, ...platformPagingFlags,
  eventType: requiredTextFlag("event-type", "Comma-separated runner event types").pipe(
    Flag.filter(value => value.split(",").every(Schema.is(Schema.Enum(RunnerEventType))), () => "expected known runner event types separated by commas"), Flag.optional,
  ),
  since: sinceFlag, until: eventTimeFlag("until", false),
}).pipe(Command.withDescription("Read runner activity (runner:read)"), guardedHandler(({id, limit, startingAfter, eventType, since, until}) =>
  platformRunnerHistory(id, RunnerHistory.Events, {limit: opt(limit), startingAfter: opt(startingAfter), eventType: opt(eventType), since: opt(since), until: opt(until)})));
const leases = Command.make("leases", {...runnerId, ...platformPagingFlags,
  workspace: requiredIdFlag("workspace", "Filter by workspace").pipe(Flag.optional),
  fleet: requiredTextFlag("fleet", "Filter by fleet ID or exact name").pipe(Flag.optional),
}).pipe(Command.withDescription("Read runner leases (runner:read)"), guardedHandler(({id, limit, startingAfter, workspace, fleet}) =>
  platformRunnerHistory(id, RunnerHistory.Leases, {limit: opt(limit), startingAfter: opt(startingAfter), workspace: opt(workspace), fleet: opt(fleet)})));

export const platformRunnersCommand = Command.make("runners").pipe(
  Command.withDescription("Manage platform execution hosts"), Command.withSubcommands([
    list, show, enroll, update, rotate, remove, events, leases,
    actionCommand("cordon", RunnerAction.Cordon), actionCommand("drain", RunnerAction.Drain), actionCommand("revoke", RunnerAction.Revoke), actionCommand("self-test", RunnerAction.SelfTest),
  ]),
);
