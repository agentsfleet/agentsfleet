import { auth } from "@clerk/nextjs/server";
import { buttonClassName, EmptyState } from "@usezombie/design-system";
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
          <div className="font-mono text-xs text-muted-foreground mb-1">
            {workspace.repo_url.replace("https://github.com/", "")}
          </div>
          <h1 className="mc-page-title">{workspace.name ?? workspace.id}</h1>
        </div>
        <div className="flex gap-2">
          {workspace.paused ? (
            <TrackedAnchor
              href={`/workspaces/${id}/resume`}
              className={buttonClassName("ghost", "sm")}
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
              className={buttonClassName("ghost", "sm")}
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
            className={buttonClassName("default", "sm")}
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
      {activeRun ? (
        <div className="mb-8">
          <p className="mb-3 font-mono text-[0.72rem] uppercase tracking-widest text-muted-foreground">
            Active run · {activeRun.id}
          </p>
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
      ) : null}

      {/* Recent runs */}
      <div className="mb-8">
        <p className="mb-3 font-mono text-[0.72rem] uppercase tracking-widest text-muted-foreground">
          Recent runs
        </p>
        {runsRes.data.length === 0 ? (
          <EmptyState
            title="No runs yet"
            description="Queue a spec to get started."
          />
        ) : (
          <div className="overflow-hidden rounded-lg border border-border">
            {runsRes.data.slice(0, 20).map((run) => (
              <RunRow key={run.id} run={run} workspaceId={id} />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
