// An uncertain write reuses its original digest and tag before any fresh review.

import type { ChatOptions } from "./commands.ts";
import type { Output } from "../../services/output.ts";
import { Effect } from "effect";
import { ValidationError } from "../../errors/index.ts";
import { renderApplied, renderFleet, isPartial, renderResult, schedulesOwed } from "./render.ts";
import { SetupSession, START } from "./setup-session.ts";
import { SETUP_ACTION, type RecoveryKey, renderSetupRecovery } from "./setup-recovery.ts";

export function recoveryKey(options: ChatOptions): RecoveryKey | undefined {
  if (!options.operationId || !options.candidateId) return undefined;
  const action = options.save ? SETUP_ACTION.save : options.apply ? SETUP_ACTION.apply : SETUP_ACTION.publish;
  return { action, key: options.operationId };
}

export function recoverSetup(setup: SetupSession, options: ChatOptions, output: Output) {
  return Effect.gen(function* () {
    const candidate = options.candidateId;
    if (!candidate) return yield* missing("--candidate-id");
    if (options.save) {
      if (!options.name || !options.reviewDigest) return yield* missing("the original --name and --review-digest");
      const start = options.runNow === undefined ? {kind: START.none} : {kind: START.runNow, message: options.runNow};
      const fleet = yield* setup.saveCandidate(candidate, options.name, options.reviewDigest, start);
      yield* renderFleet(output, fleet);
      if (isPartial(fleet) && setup.recovery) yield* renderSetupRecovery(output, setup.recovery);
      return isPartial(fleet) ? 1 : 0;
    }
    if (options.apply) {
      if (!options.fleet || !options.reviewDigest || !options.setupEtag) return yield* missing("the original --fleet, --review-digest and --setup-etag");
      const applied = yield* setup.applyCandidate(candidate, options.fleet, options.reviewDigest, options.setupEtag);
      yield* renderApplied(output, applied);
      return schedulesOwed(applied.schedules) ? 1 : 0;
    }
    if (!options.publish) return yield* missing("one explicit --save, --apply or --publish action");
    const target = options.publishTo && options.libraryEtag ? {id: options.publishTo, etag: options.libraryEtag} : undefined;
    const published = yield* setup.publishCandidate(candidate, options.publish, target);
    yield* renderResult(output, `Published Library revision ${published.revision_id}`, published);
    return 0;
  });
}

function missing(fields: string): Effect.Effect<never, ValidationError> {
  return Effect.fail(new ValidationError({detail: `Setup recovery requires ${fields}`,
    suggestion: "use the recovery metadata printed by the original invocation; keep its action arguments unchanged"}));
}
