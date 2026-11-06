import type { TaskDetail } from "@/lib/api/tasks";

/** Explain only a recorded dependency; task status alone cannot name the next action. */
export function TaskNextStep({detail}: {detail: TaskDetail}) {
  const dependency = detail.dependency;
  const target = detail.context.target;
  const outstanding = dependency?.checks.filter(check => check.status !== "completed") ?? [];
  return <div className="space-y-xs">
    <h3 className="text-label font-medium">Working toward</h3>
    <p className="whitespace-pre-wrap break-words">{detail.context.objective}</p>
    <p className="text-eyebrow text-muted-foreground">{dependency?.active
      ? outstanding.length ? `Waiting on: ${outstanding.map(check => target?.kind === "existing_pr"
        ? target.workflows.find(workflow => workflow.id === check.workflow_id)?.path ?? `Workflow ${check.workflow_id}`
        : `Workflow ${check.workflow_id}`).join(", ")}.`
        : "The recorded wait is still active. Check results are shown below; continuation is not yet confirmed."
      : "No active check dependency is recorded. Follow the activity and replies below for the next action."}</p>
  </div>;
}
