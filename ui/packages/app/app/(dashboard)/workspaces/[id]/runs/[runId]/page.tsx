import { auth } from "@clerk/nextjs/server";
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
        className="run-back"
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
      <div className="mc-page-header" style={{ marginTop: "1rem" }}>
        <div>
          <div className="run-id">{run.id}</div>
          <div className="run-spec">{run.spec_path}</div>
        </div>
        <div className="run-header-right">
          <RunStatus status={run.status} />
          {(run.status === "FAILED" || run.status === "RETRYING") && (
            <TrackedAnchor
              href={`/workspaces/${id}/runs/${runId}/retry`}
              className="run-btn-primary"
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
          )}
          {run.pr_url && (
            <TrackedAnchor
              href={run.pr_url}
              target="_blank"
              rel="noopener noreferrer"
              className="run-btn-ghost"
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
          )}
        </div>
      </div>

      {/* Pipeline */}
      <div className="run-section">
        <p className="run-section-label">Pipeline</p>
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
      <div className="run-meta-grid">
        <div className="run-meta-item">
          <span className="run-meta-label">Attempts</span>
          <span className="run-meta-value">{run.attempts} / {run.max_attempts}</span>
        </div>
        <div className="run-meta-item">
          <span className="run-meta-label">Duration</span>
          <span className="run-meta-value">
            {run.duration_seconds != null ? formatDuration(run.duration_seconds) : "—"}
          </span>
        </div>
        <div className="run-meta-item">
          <span className="run-meta-label">Created</span>
          <span className="run-meta-value">{formatDate(run.created_at)}</span>
        </div>
        <div className="run-meta-item">
          <span className="run-meta-label">Updated</span>
          <span className="run-meta-value">{formatDate(run.updated_at)}</span>
        </div>
      </div>

      {/* Artifacts */}
      {run.artifacts && (
        <div className="run-section">
          <p className="run-section-label">Artifacts</p>
          <div className="run-artifacts">
            {Object.entries(run.artifacts)
              .filter(([, v]) => v != null)
              .map(([key, value]) => (
                <div key={key} className="run-artifact">
                  <span className="run-artifact-key">{key}</span>
                  <code className="run-artifact-val">{value}</code>
                </div>
              ))}
          </div>
        </div>
      )}

      {/* Transition log */}
      <div className="run-section">
        <p className="run-section-label">Audit trail</p>
        <div className="run-transitions">
          {transitions.map((t) => (
            <div key={t.id} className="run-transition">
              <div className="run-transition-status">
                {t.from_status && <span className="t-from">{t.from_status}</span>}
                {t.from_status && <span className="t-arrow">→</span>}
                <span className="t-to">{t.to_status}</span>
              </div>
              <div className="run-transition-meta">
                <span>{t.reason}</span>
                <span className="t-dim">{t.actor} · {formatDate(t.created_at)}</span>
              </div>
            </div>
          ))}
        </div>
      </div>

      <style>{`
        .run-back {
          display: inline-flex; align-items: center; gap: 0.4rem;
          color: var(--z-text-muted); font-size: 0.82rem; text-decoration: none;
          transition: color 0.15s;
        }
        .run-back:hover { color: var(--z-text-primary); }
        .run-id {
          font-family: var(--z-font-mono); font-size: 0.75rem;
          color: var(--z-text-muted); margin-bottom: 0.2rem;
        }
        .run-spec { font-size: 1rem; font-weight: 600; }
        .run-header-right { display: flex; align-items: center; gap: 0.5rem; }
        .run-btn-primary {
          display: inline-flex; align-items: center; gap: 0.4rem;
          padding: 0.45rem 0.9rem; border-radius: var(--z-radius-pill);
          background: linear-gradient(120deg, var(--z-orange), var(--z-orange-bright));
          color: #111; font-size: 0.82rem; font-weight: 600; text-decoration: none;
        }
        .run-btn-ghost {
          display: inline-flex; align-items: center; gap: 0.4rem;
          padding: 0.45rem 0.9rem; border-radius: var(--z-radius-pill);
          border: 1px solid var(--z-border); color: var(--z-text-muted);
          font-size: 0.82rem; text-decoration: none; transition: all 0.15s;
        }
        .run-btn-ghost:hover { border-color: var(--z-orange); color: var(--z-text-primary); }
        .run-section { margin-bottom: 2rem; }
        .run-section-label {
          font-family: var(--z-font-mono); font-size: 0.7rem;
          text-transform: uppercase; letter-spacing: 0.08em;
          color: var(--z-text-muted); margin-bottom: 0.75rem;
        }
        .run-meta-grid {
          display: grid; grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
          gap: 0.75rem; margin-bottom: 2rem;
        }
        .run-meta-item {
          background: var(--z-surface-0); border: 1px solid var(--z-border);
          border-radius: var(--z-radius-md); padding: 0.75rem 1rem;
          display: flex; flex-direction: column; gap: 0.25rem;
        }
        .run-meta-label {
          font-family: var(--z-font-mono); font-size: 0.7rem;
          text-transform: uppercase; letter-spacing: 0.06em; color: var(--z-text-muted);
        }
        .run-meta-value { font-size: 0.9rem; font-weight: 600; }
        .run-artifacts { display: flex; flex-direction: column; gap: 0.4rem; }
        .run-artifact {
          display: flex; align-items: center; gap: 0.75rem;
          padding: 0.5rem 0.75rem; background: var(--z-surface-0);
          border: 1px solid var(--z-border); border-radius: var(--z-radius-sm);
        }
        .run-artifact-key {
          font-family: var(--z-font-mono); font-size: 0.72rem;
          color: var(--z-amber); text-transform: uppercase; letter-spacing: 0.04em;
          min-width: 120px;
        }
        .run-artifact-val { font-size: 0.82rem; color: var(--z-cyan); }
        .run-transitions { display: flex; flex-direction: column; gap: 0; }
        .run-transition {
          display: flex; align-items: flex-start; justify-content: space-between;
          gap: 1rem; padding: 0.75rem 0;
          border-bottom: 1px solid rgba(26, 37, 51, 0.5);
        }
        .run-transition:last-child { border-bottom: none; }
        .run-transition-status {
          display: flex; align-items: center; gap: 0.4rem;
          font-family: var(--z-font-mono); font-size: 0.75rem; white-space: nowrap;
        }
        .t-from { color: var(--z-text-muted); }
        .t-arrow { color: var(--z-text-dim); }
        .t-to { color: var(--z-cyan); font-weight: 500; }
        .run-transition-meta {
          display: flex; flex-direction: column; align-items: flex-end;
          gap: 0.15rem; font-size: 0.8rem; color: var(--z-text-muted);
        }
        .t-dim { font-family: var(--z-font-mono); font-size: 0.68rem; color: var(--z-text-dim); }
      `}</style>
    </div>
  );
}
