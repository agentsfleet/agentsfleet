import { auth } from "@clerk/nextjs/server";
import { getWorkspace, listRuns } from "@/lib/api";
import AnalyticsPageEvent from "@/components/analytics/AnalyticsPageEvent";
import RunRow from "@/components/domain/RunRow";
import PipelineStage from "@/components/domain/PipelineStage";
import TrackedAnchor from "@/components/analytics/TrackedAnchor";
import { PauseIcon, PlayIcon, RefreshCwIcon } from "lucide-react";
import { notFound } from "next/navigation";
import type { RunStatus } from "@/lib/types";

export const dynamic = "force-dynamic";

const PIPELINE_STAGES: RunStatus[] = [
  "SPEC_QUEUED",
  "RUN_PLANNED",
  "PATCH_IN_PROGRESS",
  "VERIFICATION_IN_PROGRESS",
  "PR_PREPARED",
  "PR_OPENED",
  "DONE",
];

export default async function WorkspacePage({ params }: { params: Promise<{ id: string }> }) {
  const { id } = await params;
  const { getToken } = await auth();
  const token = await getToken();

  if (!token) notFound();

  const [workspace, runsRes] = await Promise.all([
    getWorkspace(id, token),
    listRuns(id, token),
  ]).catch(() => { notFound(); }) as [Awaited<ReturnType<typeof getWorkspace>>, Awaited<ReturnType<typeof listRuns>>];

  const activeRun = runsRes.data.find((r) =>
    !["DONE", "FAILED"].includes(r.status)
  );

  return (
    <div>
      <AnalyticsPageEvent
        event="workspace_detail_viewed"
        properties={{
          source: "workspace_page",
          surface: "workspace_detail",
          workspace_id: id,
          workspace_plan: workspace.plan,
          paused: workspace.paused,
          workspace_count: runsRes.data.length,
          active_run_id: activeRun?.id,
          active_run_status: activeRun?.status,
        }}
      />
      {/* Workspace header */}
      <div className="mc-page-header">
        <div>
          <div className="ws-repo">{workspace.repo_url.replace("https://github.com/", "")}</div>
          <h1 className="mc-page-title">{workspace.name ?? workspace.id}</h1>
        </div>
        <div className="ws-actions">
          {workspace.paused ? (
            <TrackedAnchor
              href={`/workspaces/${id}/resume`}
              className="ws-btn ws-btn-ghost"
              event="workspace_action_clicked"
              properties={{
                source: "workspace_page",
                surface: "workspace_detail",
                workspace_id: id,
                target: "resume",
              }}
            >
              <PlayIcon size={14} /> Resume
            </TrackedAnchor>
          ) : (
            <TrackedAnchor
              href={`/workspaces/${id}/pause`}
              className="ws-btn ws-btn-ghost"
              event="workspace_action_clicked"
              properties={{
                source: "workspace_page",
                surface: "workspace_detail",
                workspace_id: id,
                target: "pause",
              }}
            >
              <PauseIcon size={14} /> Pause
            </TrackedAnchor>
          )}
          <TrackedAnchor
            href={`/workspaces/${id}/runs`}
            className="ws-btn ws-btn-primary"
            event="workspace_action_clicked"
            properties={{
              source: "workspace_page",
              surface: "workspace_detail",
              workspace_id: id,
              target: "all_runs",
            }}
          >
            <RefreshCwIcon size={14} /> All runs
          </TrackedAnchor>
        </div>
      </div>

      {/* Pipeline visualization for active run */}
      {activeRun && (
        <div className="ws-section">
          <p className="ws-section-label">Active run · {activeRun.id}</p>
          <div className="pipeline-track">
            {PIPELINE_STAGES.map((stage, i) => {
              const idx = PIPELINE_STAGES.indexOf(activeRun.status as RunStatus);
              const stageState =
                i < idx ? "done" : i === idx ? "active" : "pending";
              return (
                <PipelineStage
                  key={stage}
                  stage={stage}
                  state={stageState}
                  showConnector={i < PIPELINE_STAGES.length - 1}
                />
              );
            })}
          </div>
        </div>
      )}

      {/* Recent runs */}
      <div className="ws-section">
        <p className="ws-section-label">Recent runs</p>
        {runsRes.data.length === 0 ? (
          <div className="mc-empty">
            <p>No runs yet. Queue a spec to get started.</p>
          </div>
        ) : (
          <div className="runs-table">
            {runsRes.data.slice(0, 20).map((run) => (
              <RunRow key={run.id} run={run} workspaceId={id} />
            ))}
          </div>
        )}
      </div>

      <style>{`
        .ws-repo {
          font-family: var(--z-font-mono);
          font-size: 0.75rem;
          color: var(--z-text-muted);
          margin-bottom: 0.2rem;
        }
        .ws-actions { display: flex; gap: 0.5rem; }
        .ws-btn {
          display: inline-flex; align-items: center; gap: 0.4rem;
          padding: 0.45rem 0.9rem; border-radius: var(--z-radius-pill);
          font-size: 0.82rem; font-weight: 500; text-decoration: none;
          transition: all 0.15s;
        }
        .ws-btn-ghost {
          border: 1px solid var(--z-border); color: var(--z-text-muted);
        }
        .ws-btn-ghost:hover { border-color: var(--z-orange); color: var(--z-text-primary); }
        .ws-btn-primary {
          background: linear-gradient(120deg, var(--z-orange), var(--z-orange-bright));
          color: #111; font-weight: 600;
        }
        .ws-section { margin-bottom: 2rem; }
        .ws-section-label {
          font-family: var(--z-font-mono); font-size: 0.72rem;
          text-transform: uppercase; letter-spacing: 0.08em;
          color: var(--z-text-muted); margin-bottom: 0.75rem;
        }
        .runs-table {
          border: 1px solid var(--z-border); border-radius: var(--z-radius-lg);
          overflow: hidden;
        }
      `}</style>
    </div>
  );
}
