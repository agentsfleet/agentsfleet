"use client";

import { latestTaskActivity, readTaskActivityHistory, taskActivityLabel } from "@/lib/task-activity";

/** Public progress stays separate from private model reasoning and native task status. */
export function TaskActivityTimeline({value, running}: {value: unknown; running: boolean}) {
  const history = readTaskActivityHistory(value);
  if (!history || history.items.length === 0) return null;
  const latest = latestTaskActivity(history).at(-1);
  if (!latest) return null;
  const pending = ["starting", "preparing", "uploading", "verifying", "restoring"].includes(latest.progress.phase);
  return <details className="rounded-lg border border-border px-md py-sm text-body-sm text-muted-foreground">
    <summary className="cursor-pointer break-words">{taskActivityLabel(latest)}{pending && !running ? " · no confirmed completion recorded" : ""}</summary>
    <ol className="mt-sm space-y-sm" aria-label="Task activity">
      {history.items.map(item => <li key={`${item.kind}:${item.progress.activity_id}:${item.progress.operation}:${item.progress.phase}`}>
        <p>{taskActivityLabel(item)}</p>
        {item.progress.total === null ? null : <p className="text-eyebrow">{item.progress.completed} of {item.progress.total} objects</p>}
      </li>)}
    </ol>
    {history.truncated ? <p className="mt-sm text-eyebrow">Earlier progress stages are outside the retained activity window.</p> : null}
  </details>;
}
