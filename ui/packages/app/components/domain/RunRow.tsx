"use client";

import Link from "next/link";
import type { Run } from "@/lib/types";
import { trackAppEvent } from "@/lib/analytics/posthog";
import RunStatus from "./RunStatus";
import { formatDate, formatDuration, truncate } from "@/lib/utils";
import { ExternalLinkIcon } from "lucide-react";

type Props = {
  run: Run;
  workspaceId: string;
};

export default function RunRow({ run, workspaceId }: Props) {
  return (
    <Link
      href={`/workspaces/${workspaceId}/runs/${run.id}`}
      style={{ textDecoration: "none" }}
      onClick={() =>
        trackAppEvent("run_opened", {
          source: "run_row",
          surface: "workspace_runs",
          workspace_id: workspaceId,
          run_id: run.id,
          run_status: run.status,
          has_pr_url: Boolean(run.pr_url),
        })
      }
    >
      <div className="run-row">
        <div className="run-row-left">
          <RunStatus status={run.status} size="sm" />
          <div className="run-row-spec">{truncate(run.spec_path, 60)}</div>
        </div>

        <div className="run-row-right">
          {run.pr_url && (
            <a
              href={run.pr_url}
              target="_blank"
              rel="noopener noreferrer"
              className="run-row-pr"
              onClick={(e) => {
                e.stopPropagation();
                trackAppEvent("run_pr_clicked", {
                  source: "run_row",
                  surface: "workspace_runs",
                  workspace_id: workspaceId,
                  run_id: run.id,
                  target: run.pr_url || undefined,
                });
              }}
            >
              PR <ExternalLinkIcon size={10} />
            </a>
          )}
          <span className="run-row-meta">
            {run.duration_seconds != null
              ? formatDuration(run.duration_seconds)
              : "—"}
          </span>
          <span className="run-row-meta">{formatDate(run.created_at)}</span>
        </div>
      </div>

      <style>{`
        .run-row {
          display: flex;
          align-items: center;
          justify-content: space-between;
          padding: 0.75rem 1rem;
          border-bottom: 1px solid rgba(26, 37, 51, 0.6);
          transition: background 0.15s;
          cursor: pointer;
        }
        .run-row:last-child { border-bottom: none; }
        .run-row:hover { background: var(--z-surface-1); }
        .run-row-left { display: flex; align-items: center; gap: 0.75rem; }
        .run-row-spec {
          font-family: var(--z-font-mono);
          font-size: 0.78rem;
          color: var(--z-text-primary);
        }
        .run-row-right {
          display: flex;
          align-items: center;
          gap: 0.75rem;
        }
        .run-row-pr {
          display: inline-flex;
          align-items: center;
          gap: 0.25rem;
          font-family: var(--z-font-mono);
          font-size: 0.7rem;
          color: var(--z-cyan);
          text-decoration: none;
          transition: opacity 0.15s;
        }
        .run-row-pr:hover { opacity: 0.8; }
        .run-row-meta {
          font-family: var(--z-font-mono);
          font-size: 0.72rem;
          color: var(--z-text-muted);
          white-space: nowrap;
        }
      `}</style>
    </Link>
  );
}
