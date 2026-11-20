import {RunnerAction, RunnerEventType, RunnerHistory} from "../../commands/platform/runner-vocabulary.ts";
import { Schema } from "effect";
import { Command, Flag } from "effect/cli";
import { runnerAction, runnerDelete, runnerHistory, runnerRegister, runnerRotate, runnersList, runnerShow, runnerUpdate } from "../../commands/platform/runners.ts";
import { PLATFORM_RUNNERS, TENANT_RUNNERS, type RunnerKind } from "../../commands/platform/runner-kind.ts";
import { guardedHandler } from "./guarded-handler.ts";
import { grants, listedWhen, type Visibility } from "./visibility.ts";
import { platformLabelsFlag, platformPagingFlags, platformPolicyFlag, platformTokenFlag } from "./platform-flags.ts";
import { eventTimeFlag, idArgument, opt, requiredIdFlag, requiredTextFlag, sinceFlag } from "./flags.ts";

const runnerId = {id: idArgument("runner_id", "Runner identifier")};

/**
 * One command group per kind of runner, from one definition: the platform's
 * `platform runners` and a tenant's own `runners` share every handler, and
 * differ only in the API path, the scopes that list each verb, and the
 * operator-only history verbs.
 */
export const runnersCommand = (kind: RunnerKind, visibility: Visibility) => {
  const {read: readScope, write: writeScope, register: registerScope} = kind.scopes;
  const actionCommand = (name: string, action: RunnerAction) => Command.make(name, runnerId).pipe(
    Command.withDescription(`Request ${name} (${writeScope})`), guardedHandler(({id}) => runnerAction(kind, id, action)),
  );
  const list = Command.make("list", platformPagingFlags).pipe(
    Command.withDescription(`List runners (${readScope})`),
    guardedHandler(({limit, startingAfter}) => runnersList(kind, {limit: opt(limit), startingAfter: opt(startingAfter)})),
  );
  const show = Command.make("show", runnerId).pipe(
    Command.withDescription(`Show policy, capabilities, liveness, counters, and checks (${readScope})`),
    guardedHandler(({id}) => runnerShow(kind, id)),
  );
  const register = Command.make("register", {
    host: requiredTextFlag("host-id", "Host identity", "<host>"), policy: platformPolicyFlag,
    labels: platformLabelsFlag, tokenFile: platformTokenFlag,
  }).pipe(Command.withDescription(`Register a host and save its token (${registerScope})`),
    guardedHandler(({host, policy, labels, tokenFile}) => runnerRegister(kind, host, {policy, labels: opt(labels)}, tokenFile)));
  const update = Command.make("update", {...runnerId, policy: platformPolicyFlag.pipe(Flag.optional), labels: platformLabelsFlag}).pipe(
    Command.withDescription(`Replace policy and/or labels (${writeScope})`),
    guardedHandler(({id, policy, labels}) => runnerUpdate(kind, id, {policy: opt(policy), labels: opt(labels)})),
  );
  const rotate = Command.make("rotate-token", {...runnerId, tokenFile: platformTokenFlag}).pipe(
    Command.withDescription(`Replace the runner token and save it once (${writeScope})`),
    guardedHandler(({id, tokenFile}) => runnerRotate(kind, id, tokenFile)),
  );
  const remove = Command.make("delete", runnerId).pipe(
    Command.withDescription(`Delete a revoked runner and its history (${writeScope})`), guardedHandler(({id}) => runnerDelete(kind, id)),
  );
  const history = Command.make("history", {...runnerId, ...platformPagingFlags,
    eventType: requiredTextFlag("event-type", "Comma-separated runner event types").pipe(
      Flag.filter(value => value.split(",").every(Schema.is(Schema.Enum(RunnerEventType))), () => "expected known runner event types separated by commas"), Flag.optional,
    ),
    since: sinceFlag, until: eventTimeFlag("until", false),
  }).pipe(Command.withDescription(`Read runner history (${readScope})`), guardedHandler(({id, limit, startingAfter, eventType, since, until}) =>
    runnerHistory(id, RunnerHistory.History, {limit: opt(limit), startingAfter: opt(startingAfter), eventType: opt(eventType), since: opt(since), until: opt(until)})));
  const runs = Command.make("runs", {...runnerId, ...platformPagingFlags,
    workspace: requiredIdFlag("workspace", "Filter by workspace").pipe(Flag.optional),
    fleet: requiredTextFlag("fleet", "Filter by fleet ID or exact name").pipe(Flag.optional),
  }).pipe(Command.withDescription(`Read runner runs (${readScope})`), guardedHandler(({id, limit, startingAfter, workspace, fleet}) =>
    runnerHistory(id, RunnerHistory.Runs, {limit: opt(limit), startingAfter: opt(startingAfter), workspace: opt(workspace), fleet: opt(fleet)})));

  // Registering is independent of the read/write ladder: a registrar sees
  // `register` and nothing else unless the server also reported read.
  const read = grants(visibility, readScope);
  const registrable = grants(visibility, registerScope);
  const write = grants(visibility, writeScope);
  const reads = listedWhen(read);
  const writes = listedWhen(write);
  const operatorReads = kind.history ? [reads(history), reads(runs)] : [];
  return {
    listed: read || registrable || write,
    command: Command.make("runners").pipe(
      Command.withDescription(kind.description), Command.withSubcommands([
        reads(list), reads(show), listedWhen(registrable)(register), writes(update), writes(rotate), writes(remove),
        ...operatorReads,
        writes(actionCommand("cordon", RunnerAction.Cordon)), writes(actionCommand("drain", RunnerAction.Drain)),
        writes(actionCommand("revoke", RunnerAction.Revoke)), writes(actionCommand("self-test", RunnerAction.SelfTest)),
      ]),
    ),
  };
};

export const platformRunnersCommand = (visibility: Visibility) => runnersCommand(PLATFORM_RUNNERS, visibility);

export const tenantRunnersCommand = (visibility: Visibility) => runnersCommand(TENANT_RUNNERS, visibility);
