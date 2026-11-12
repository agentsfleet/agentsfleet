"use client";

import { ArrowUpRightIcon, GitPullRequestIcon } from "@phosphor-icons/react/ssr";
import { Card, Skeleton } from "@agentsfleet/design-system";
import { readTaskActivityHistory } from "@/lib/task-activity";
import { useFleetTaskDetails, useFleetTaskView, type FleetTaskReadState } from "./FleetTaskProvider";
import { accessLost, confirmed } from "./use-task-read";

/** An explicit single native task identity binds this reply to a saved PR target. */
export function NativeTaskPrBadge({activity, appearance = "badge"}: {activity: unknown; appearance?: "badge" | "card"}) {
  const details = useFleetTaskDetails();
  const view = useFleetTaskView();
  const history = readTaskActivityHistory(activity);
  const ids = new Set(history?.items.flatMap(item => item.progress.task_id ? [item.progress.task_id] : []));
  const entry = ids.values().next();
  if (history?.truncated || entry.done || ids.size > 1) return null;
  const resource = details.get(entry.value);
  if (appearance === "card" && view?.subject && waitingForLinkedWork(view.read, entry.value)) {
    return <output className="mt-xl block" aria-label="Reading linked work"><Skeleton className="h-19.5 w-full rounded-md" aria-hidden="true" /></output>;
  }
  if (!resource) return null;
  const detail = confirmed(resource);
  const target = detail?.context.target;
  if (!detail || target?.kind !== "existing_pr") return null;
  if (appearance === "card") return <Card asChild className="mt-xl flex h-19.5 items-center gap-lg rounded-md p-xl">
    <a href={target.url} target="_blank" rel="noopener noreferrer" className="ph-no-capture ph-no-record"
      aria-label={`Pull request #${target.number} · ${target.repository} · ${detail.title}`}>
      <GitPullRequestIcon size={16} className="shrink-0 text-muted-foreground" aria-hidden="true" />
      <span className="min-w-0 flex-1">
        <span className="block truncate text-label font-medium leading-label" title={detail.title}>{detail.title}</span>
        <span className="mt-sm flex min-w-0 items-center gap-sm text-eyebrow leading-eyebrow text-muted-foreground">
          <span className="shrink-0">Pull request #{target.number}</span><span aria-hidden="true"> · </span>
          <span className="truncate" title={target.repository}>{target.repository}</span>
          {resource.kind !== "ready" ? <span className="shrink-0"> · last confirmed</span> : null}
        </span>
      </span>
      <ArrowUpRightIcon size={16} className="shrink-0 text-muted-foreground" aria-hidden="true" />
    </a>
  </Card>;
  return <a href={target.url} target="_blank" rel="noopener noreferrer"
    aria-label={`Pull request #${target.number} · ${target.repository} · ${detail.title}`}
    title={`${detail.title} · ${target.repository}${resource.kind !== "ready" ? " · last confirmed" : ""}`}
    className="ph-no-capture ph-no-record inline-flex h-5 shrink-0 items-center gap-xs rounded-md border border-border px-sm text-body-sm text-muted-foreground hover:text-foreground">
    <GitPullRequestIcon size={14} aria-hidden="true" />#{target.number}
  </a>;
}

/** Only the shared reader's selected work can reserve a pending result. */
function waitingForLinkedWork(read: FleetTaskReadState | null, taskId: string): boolean {
  if (!read) return true;
  if (accessLost(read.resource)) return false;
  if (read.resource.kind === "loading" && !confirmed(read.resource)) return true;
  if (!read.references.some(reference => reference.id === taskId)) return false;
  const resource = read.details.get(taskId);
  return !resource || resource.kind === "loading" && !confirmed(resource);
}
