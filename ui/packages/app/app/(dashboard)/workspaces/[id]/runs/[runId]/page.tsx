import { auth } from "@clerk/nextjs/server";
import { buttonClassName, StatusCard } from "@usezombie/design-system";
import { getRun, listRunTransitions } from "@/lib/api";
import AnalyticsPageEvent from "@/components/analytics/AnalyticsPageEvent";
import RunStatus from "@/components/domain/RunStatus";
import PipelineStage from "@/components/domain/PipelineStage";
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
      {/* Back nav */}
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

      {/* Run header */}
      <div className="mc-page-header mt-4">
        <div>
          <div className="font-mono text-xs text-muted-foreground mb-1">{run.id}</div>
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
      </div>

      {/* Pipeline */}
      <div className="mb-8">
        <p className="mb-3 font-mono text-[0.7rem] uppercase tracking-widest text-muted-foreground">
          Pipeline
        </p>
        <div className="pipeline-track">
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
      </div>

      {/* Metadata */}
      <div className="mb-8 grid gap-3" style={{ gridTemplateColumns: "repeat(auto-fit, minmax(180px, 1fr))" }}>
        <StatusCard label="Attempts" count={`${run.attempts} / ${run.max_attempts}`} />
        <StatusCard
          label="Duration"
          count={run.duration_seconds != null ? formatDuration(run.duration_seconds) : "—"}
        />
        <StatusCard label="Created" count={formatDate(run.created_at)} />
        <StatusCard label="Updated" count={formatDate(run.updated_at)} />
      </div>

      {/* Artifacts */}
      {run.artifacts ? (
        <div className="mb-8">
          <p className="mb-3 font-mono text-[0.7rem] uppercase tracking-widest text-muted-foreground">
            Artifacts
          </p>
          <div className="flex flex-col gap-1.5">
            {Object.entries(run.artifacts)
              .filter(([, v]) => v != null)
              .map(([key, value]) => (
                <div
                  key={key}
                  className="flex items-center gap-3 rounded-sm border border-border bg-card px-3 py-2"
                >
                  <span className="min-w-[120px] font-mono text-[0.72rem] uppercase tracking-wide text-warning">
                    {key}
                  </span>
                  <code className="text-xs text-info">{value}</code>
                </div>
              ))}
          </div>
        </div>
      ) : null}

      {/* Transition log */}
      <div className="mb-8">
        <p className="mb-3 font-mono text-[0.7rem] uppercase tracking-widest text-muted-foreground">
          Audit trail
        </p>
        <div className="flex flex-col">
          {transitions.map((t) => (
            <div
              key={t.id}
              className="flex items-start justify-between gap-4 border-b border-border/50 py-3 last:border-b-0"
            >
              <div className="flex items-center gap-2 whitespace-nowrap font-mono text-xs">
                {t.from_status ? (
                  <>
                    <span className="text-muted-foreground">{t.from_status}</span>
                    <span className="text-muted-foreground/60">→</span>
                  </>
                ) : null}
                <span className="font-medium text-info">{t.to_status}</span>
              </div>
              <div className="flex flex-col items-end gap-0.5 text-sm text-muted-foreground">
                <span>{t.reason}</span>
                <span className="font-mono text-[0.68rem] text-muted-foreground/60">
                  {t.actor} · {formatDate(t.created_at)}
                </span>
              </div>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
