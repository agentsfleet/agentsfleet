import { auth } from "@clerk/nextjs/server";
import {
  buttonClassName,
  PageHeader,
  SectionLabel,
  StatusCard,
} from "@usezombie/design-system";
import { getRun, listRunTransitions } from "@/lib/api";
import AnalyticsPageEvent from "@/components/analytics/AnalyticsPageEvent";
import RunStatus from "@/components/domain/RunStatus";
import PipelineStage from "@/components/domain/PipelineStage";
import ArtifactRow from "@/components/domain/ArtifactRow";
import TransitionRow from "@/components/domain/TransitionRow";
import { notFound } from "next/navigation";
import { formatDate, formatDuration } from "@/lib/utils";
import type { RunStatus as RunStatusType } from "@/lib/types";
import TrackedAnchor from "@/components/analytics/TrackedAnchor";
import { ArrowLeftIcon, ExternalLinkIcon, RefreshCwIcon } from "lucide-react";

export const dynamic = "force-dynamic";

const PIPELINE_STAGES: RunStatusType[] = [
  "SPEC_QUEUED",
  "RUN_PLANNED",
  "PATCH_IN_PROGRESS",
  "VERIFICATION_IN_PROGRESS",
  "PR_PREPARED",
  "PR_OPENED",
  "DONE",
];

export default async function RunDetailPage({
  params,
}: {
  params: Promise<{ id: string; runId: string }>;
}) {
  const { id, runId } = await params;
  const { getToken } = await auth();
  const token = await getToken();

  if (!token) notFound();

  const [run, transitions] = await Promise.all([
    getRun(runId, token),
    listRunTransitions(runId, token),
  ]).catch(() => { notFound(); }) as [Awaited<ReturnType<typeof getRun>>, Awaited<ReturnType<typeof listRunTransitions>>];

  const currentStageIdx = PIPELINE_STAGES.indexOf(run.status as RunStatusType);

  return (
    <div>
      <AnalyticsPageEvent
        event="run_detail_viewed"
        properties={{
          source: "run_page",
          surface: "run_detail",
          workspace_id: id,
          run_id: runId,
          run_status: run.status,
          run_attempts: run.attempts,
          has_error: Boolean(run.error),
          has_pr_url: Boolean(run.pr_url),
        }}
      />
      {run.error ? (
        <AnalyticsPageEvent
          event="run_error_viewed"
          properties={{
            source: "run_page",
            surface: "run_detail",
            workspace_id: id,
            run_id: runId,
            run_status: run.status,
            error_message: run.error,
          }}
        />
      ) : null}

      <TrackedAnchor
        href={`/workspaces/${id}`}
        className="inline-flex items-center gap-2 text-sm text-muted-foreground transition-colors hover:text-foreground"
        event="run_navigation_clicked"
        properties={{
          source: "run_page",
          surface: "run_detail",
          workspace_id: id,
          run_id: runId,
          target: "workspace_back",
        }}
      >
        <ArrowLeftIcon size={14} /> Workspace
      </TrackedAnchor>

      <PageHeader className="mt-4">
        <div>
          <div className="mb-1 font-mono text-xs text-muted-foreground">{run.id}</div>
          <div className="text-base font-semibold">{run.spec_path}</div>
        </div>
        <div className="flex items-center gap-2">
          <RunStatus status={run.status} />
          {run.status === "FAILED" || run.status === "RETRYING" ? (
            <TrackedAnchor
              href={`/workspaces/${id}/runs/${runId}/retry`}
              className={buttonClassName("default", "sm")}
              event="run_retry_clicked"
              properties={{
                source: "run_page",
                surface: "run_detail",
                workspace_id: id,
                run_id: runId,
                run_status: run.status,
              }}
            >
              <RefreshCwIcon size={13} /> Retry
            </TrackedAnchor>
          ) : null}
          {run.pr_url ? (
            <TrackedAnchor
              href={run.pr_url}
              target="_blank"
              rel="noopener noreferrer"
              className={buttonClassName("ghost", "sm")}
              event="run_pr_clicked"
              properties={{
                source: "run_page",
                surface: "run_detail",
                workspace_id: id,
                run_id: runId,
                target: run.pr_url,
              }}
            >
              View PR <ExternalLinkIcon size={13} />
            </TrackedAnchor>
          ) : null}
        </div>
      </PageHeader>

      <section className="mb-8">
        <SectionLabel>Pipeline</SectionLabel>
        <div className="flex items-center gap-0 overflow-x-auto py-4">
          {PIPELINE_STAGES.map((stage, i) => {
            const state =
              i < currentStageIdx ? "done"
              : i === currentStageIdx ? (run.status === "FAILED" ? "failed" : "active")
              : "pending";
            return (
              <PipelineStage
                key={stage}
                stage={stage}
                state={state}
                showConnector={i < PIPELINE_STAGES.length - 1}
              />
            );
          })}
        </div>
      </section>

      <section className="mb-8 grid grid-cols-[repeat(auto-fit,minmax(180px,1fr))] gap-3">
        <StatusCard label="Attempts" count={`${run.attempts} / ${run.max_attempts}`} />
        <StatusCard
          label="Duration"
          count={run.duration_seconds != null ? formatDuration(run.duration_seconds) : "—"}
        />
        <StatusCard label="Created" count={formatDate(run.created_at)} />
        <StatusCard label="Updated" count={formatDate(run.updated_at)} />
      </section>

      {run.artifacts ? (
        <section className="mb-8">
          <SectionLabel>Artifacts</SectionLabel>
          <div className="flex flex-col gap-1.5">
            {Object.entries(run.artifacts)
              .filter(([, v]) => v != null)
              .map(([key, value]) => (
                <ArtifactRow key={key} label={key} value={String(value)} />
              ))}
          </div>
        </section>
      ) : null}

      <section className="mb-8">
        <SectionLabel>Audit trail</SectionLabel>
        <div className="flex flex-col">
          {transitions.map((t) => (
            <TransitionRow
              key={t.id}
              fromStatus={t.from_status}
              toStatus={t.to_status}
              reason={t.reason}
              actor={t.actor}
              createdAt={t.created_at}
            />
          ))}
        </div>
      </section>
    </div>
  );
}
