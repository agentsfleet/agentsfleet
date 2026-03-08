import Link from "next/link";
import type { Workspace } from "@/lib/types";
import { formatDate } from "@/lib/utils";
import { PauseIcon, GitBranchIcon, ActivityIcon } from "lucide-react";

type Props = { workspace: Workspace };

const PLAN_LABELS: Record<Workspace["plan"], string> = {
  hobby:      "Hobby",
  pro:        "Pro",
  team:       "Team",
  enterprise: "Enterprise",
};

export default function WorkspaceCard({ workspace }: Props) {
  const repoShort = workspace.repo_url.replace(/^https?:\/\/[^/]+\//, "");

  return (
    <Link href={`/workspaces/${workspace.id}`} style={{ textDecoration: "none" }}>
      <article className={`ws-card${workspace.paused ? " ws-card--paused" : ""}`}>
        <div className="ws-card-top">
          <div className="ws-card-repo">
            <GitBranchIcon size={13} />
            {repoShort}
          </div>
          <div className="ws-card-badges">
            <span className="ws-plan-badge">{PLAN_LABELS[workspace.plan]}</span>
            {workspace.paused && (
              <span className="ws-paused-badge">
                <PauseIcon size={10} /> Paused
              </span>
            )}
          </div>
        </div>

        <div className="ws-card-stats">
          <div className="ws-stat">
            <ActivityIcon size={12} />
            <span>{workspace.run_count} runs</span>
          </div>
          {workspace.last_run_at && (
            <div className="ws-stat ws-stat--muted">
              Last: {formatDate(workspace.last_run_at)}
            </div>
          )}
        </div>
      </article>

      <style>{`
        .ws-card {
          padding: 1rem 1.25rem;
          background: var(--z-surface-0);
          border: 1px solid var(--z-border);
          border-radius: var(--z-radius-lg);
          transition: border-color 0.2s, box-shadow 0.2s;
          cursor: pointer;
        }
        .ws-card:hover {
          border-color: rgba(255, 107, 53, 0.3);
          box-shadow: 0 0 24px var(--z-glow-orange);
        }
        .ws-card--paused {
          opacity: 0.7;
          border-style: dashed;
        }
        .ws-card-top {
          display: flex;
          align-items: flex-start;
          justify-content: space-between;
          gap: 0.5rem;
          margin-bottom: 0.75rem;
        }
        .ws-card-repo {
          display: flex;
          align-items: center;
          gap: 0.35rem;
          font-family: var(--z-font-mono);
          font-size: 0.8rem;
          color: var(--z-text-primary);
          font-weight: 500;
        }
        .ws-card-badges { display: flex; align-items: center; gap: 0.4rem; }
        .ws-plan-badge {
          padding: 0.15rem 0.5rem;
          border-radius: var(--z-radius-pill);
          border: 1px solid rgba(255, 190, 46, 0.2);
          background: rgba(255, 190, 46, 0.06);
          color: var(--z-amber);
          font-family: var(--z-font-mono);
          font-size: 0.65rem;
          text-transform: uppercase;
          letter-spacing: 0.04em;
        }
        .ws-paused-badge {
          display: inline-flex; align-items: center; gap: 0.25rem;
          padding: 0.15rem 0.5rem;
          border-radius: var(--z-radius-pill);
          background: rgba(255, 190, 46, 0.08);
          color: var(--z-amber);
          font-family: var(--z-font-mono);
          font-size: 0.65rem;
        }
        .ws-card-stats {
          display: flex;
          align-items: center;
          gap: 1rem;
        }
        .ws-stat {
          display: flex;
          align-items: center;
          gap: 0.3rem;
          font-size: 0.78rem;
          color: var(--z-text-muted);
        }
        .ws-stat--muted { font-family: var(--z-font-mono); font-size: 0.72rem; }
      `}</style>
    </Link>
  );
}
