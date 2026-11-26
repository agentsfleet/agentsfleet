import type { DurableTaskDetail } from "@/lib/api/tasks";

/** Explain only a recorded dependency; task status alone cannot name the next action. */
export function DurableTaskNextStep({detail}: {detail: DurableTaskDetail}) {
  const dependency = detail.dependency;
  const target = detail.context.target;
  const outstanding = dependency?.checks.filter(check => check.status !== "completed") ?? [];
  return <div className="space-y-xs">
    <h3 className="text-label font-semibold leading-label text-text-dim">Task goal</h3>
    <p className="whitespace-pre-wrap break-words">{detail.context.objective}</p>
    {dependency?.active ? <p className="text-eyebrow text-muted-foreground">{
      outstanding.length ? `Waiting for ${outstanding.map(check => target?.kind === "existing_pr"
        ? target.workflows.find(workflow => workflow.id === check.workflow_id)?.path ?? `Workflow ${check.workflow_id}`
        : `Workflow ${check.workflow_id}`).join(", ")}.`
        : "Waiting for the fleet to confirm the checks."}</p> : null}
  </div>;
}
