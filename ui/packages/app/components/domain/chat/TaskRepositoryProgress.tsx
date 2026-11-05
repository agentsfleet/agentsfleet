import { Button, Time } from "@agentsfleet/design-system";
import type { TaskDetail } from "@/lib/api/tasks";

/** Native saved evidence stays distinct from local file edits and new checks. */
export function TaskRepositoryProgress({detail}: {detail: TaskDetail}) {
  const target = detail.context.target;
  if (!target || target.kind !== "existing_pr") return null;
  const dependency = detail.dependency;
  const publication = detail.publication;
  return <div className="space-y-sm break-words" aria-label="Repository progress">
    <Button asChild variant="link" size="sm" className="h-auto whitespace-normal p-0">
      <a href={target.url} target="_blank" rel="noopener noreferrer">{target.repository} #{target.number}</a>
    </Button>
    <p className="text-eyebrow">Confirmed repository head · <span className="font-mono break-all">{target.head}</span></p>
    <p className="text-eyebrow text-muted-foreground">Local edits and saved recovery files are not a confirmed publication.</p>
    {publication ? <div className="space-y-xs text-eyebrow" aria-label="Publication receipt">
      <p>{publication.state === "confirmed" && publication.commit ? "Published head" : `Publication ${publication.state} · no confirmed published head`}
        {publication.state === "confirmed" && publication.commit ? <> · <span className="font-mono break-all">{publication.commit}</span></> : null}</p>
      <p>Expected parent · <span className="font-mono break-all">{publication.expected_head}</span></p>
      <p>Tested tree · <span className="font-mono break-all">{publication.tested_tree}</span></p>
    </div> : null}
    {dependency ? <>
      <p className="font-medium">{dependency.active ? "Waiting for reviewed checks" : "Saved check results"}</p>
      <p className="text-eyebrow">Dependency head · <span className="font-mono break-all">{dependency.head}</span></p>
      <p className="text-eyebrow text-muted-foreground">Wait updated <Time value={dependency.updated_at} />. These are saved results, not a fresh remote check.</p>
      <ul className="space-y-xs" aria-label="Saved check results">
        {dependency.checks.map(check => <li key={check.workflow_id}>
          <span>{target.workflows.find(workflow => workflow.id === check.workflow_id)?.path ?? `Workflow ${check.workflow_id}`}</span>
          <span> · {check.status}{check.conclusion ? ` · ${check.conclusion}` : " · no conclusion"}</span>
          {check.run_id ? <span className="text-eyebrow"> · run {check.run_id}, attempt {check.run_attempt ?? "unknown"}</span> : null}
        </li>)}
      </ul>
    </> : <p className="text-eyebrow text-muted-foreground">No saved check dependency is recorded.</p>}
  </div>;
}
