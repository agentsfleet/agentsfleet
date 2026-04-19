import { auth } from "@clerk/nextjs/server";
import {
  buttonClassName,
  EmptyState,
  PageHeader,
  PageTitle,
  SectionLabel,
} from "@usezombie/design-system";
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

      <PageHeader>
        <div>
          <div className="mb-1 font-mono text-xs text-muted-foreground">
            {workspace.repo_url.replace("https://github.com/", "")}
          </div>
          <PageTitle>{workspace.name ?? workspace.id}</PageTitle>
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
      </PageHeader>

      {activeRun ? (
        <section className="mb-8">
          <SectionLabel>Active run · {activeRun.id}</SectionLabel>
          <div className="flex items-center gap-0 overflow-x-auto py-4">
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
        </section>
      ) : null}

      <section className="mb-8">
        <SectionLabel>Recent runs</SectionLabel>
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
      </section>
    </div>
  );
}
