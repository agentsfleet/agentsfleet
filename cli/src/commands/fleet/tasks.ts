// Saved repository evidence enriches steer without another execution/control path.

import { Effect } from "effect";
import { Output } from "../../services/output.ts";
import type { TaskDetail } from "./task-read.ts";
import { activityText } from "./task-activity.ts";

export function renderRepository(detail: TaskDetail) {
  return Effect.gen(function* () {
    const output = yield* Output;
    const target = detail.context.target;
    if (!target || target.kind !== "existing_pr") return;
    yield* output.info(activityText(`Pull request: ${target.url}\nConfirmed repository head: ${target.head}\nLocal edits and saved recovery files are not a confirmed publication.`));
    const publication = detail.publication;
    if (publication) {
      yield* output.info(publication.state === "confirmed" && publication.commit
        ? `Published head: ${publication.commit}` : `Publication ${publication.state}: no confirmed published head`);
      yield* output.info(`Expected parent: ${publication.expected_head}\nTested tree: ${publication.tested_tree}`);
    }
    const dependency = detail.dependency;
    if (!dependency) return yield* output.info("No saved check dependency is recorded.");
    yield* output.info(`${dependency.active ? "Waiting for reviewed checks" : "Saved check results"} at ${dependency.head}\nWait updated ${dependency.updated_at}; not a fresh remote check.`);
    for (const check of dependency.checks) {
      const path = target.workflows.find(workflow => workflow.id === check.workflow_id)?.path ?? `Workflow ${check.workflow_id}`;
      yield* output.info(activityText(`${path}: ${check.status} / ${check.conclusion ?? "no conclusion"}${check.run_id === null ? "" : ` · run ${check.run_id}, attempt ${check.run_attempt ?? "unknown"}`}`));
    }
  });
}
