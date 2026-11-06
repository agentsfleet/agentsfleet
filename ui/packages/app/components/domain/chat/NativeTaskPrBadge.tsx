"use client";

import { GitPullRequest } from "lucide-react";
import { readTaskActivityHistory } from "@/lib/task-activity";
import { useFleetTaskDetails } from "./FleetTaskProvider";
import { confirmed } from "./use-task-read";

/** An explicit single native task identity binds this reply to a saved PR target. */
export function NativeTaskPrBadge({activity}: {activity: unknown}) {
  const details = useFleetTaskDetails();
  const history = readTaskActivityHistory(activity);
  const ids = new Set(history?.items.flatMap(item => item.progress.task_id ? [item.progress.task_id] : []));
  const entry = ids.values().next();
  if (history?.truncated || entry.done || ids.size > 1) return null;
  const resource = details.get(entry.value);
  if (!resource) return null;
  const detail = confirmed(resource);
  const target = detail?.context.target;
  if (!detail || target?.kind !== "existing_pr") return null;
  return <a href={target.url} target="_blank" rel="noopener noreferrer"
    aria-label={`Pull request #${target.number} · ${target.repository} · ${detail.title}`}
    title={`${detail.title} · ${target.repository}${resource.kind !== "ready" ? " · last confirmed" : ""}`}
    className="ph-no-capture ph-no-record inline-flex h-5 shrink-0 items-center gap-xs rounded-md border border-border px-sm text-body-sm text-muted-foreground hover:text-foreground">
    <GitPullRequest size={14} aria-hidden="true" />#{target.number}
  </a>;
}
