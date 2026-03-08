import { auth } from "@clerk/nextjs/server";
import { listWorkspaces } from "@/lib/api";
import WorkspaceCard from "@/components/domain/WorkspaceCard";
import { PlusIcon } from "lucide-react";

export const dynamic = "force-dynamic";

export default async function WorkspacesPage() {
  const { getToken } = await auth();
  const token = await getToken();

  let workspaces = null;
  let error: string | null = null;

  try {
    if (token) {
      const res = await listWorkspaces(token);
      workspaces = res.data;
    }
  } catch (e) {
    error = e instanceof Error ? e.message : "Failed to load workspaces";
  }

  return (
    <div>
      <div className="mc-page-header">
        <h1 className="mc-page-title">Workspaces</h1>
        <a
          href="https://docs.usezombie.com/quickstart#add-workspace"
          target="_blank"
          rel="noopener noreferrer"
          className="mc-btn-primary"
        >
          <PlusIcon size={14} />
          Add workspace
        </a>
      </div>

      {error && (
        <div className="mc-error-banner">{error}</div>
      )}

      {workspaces && workspaces.length === 0 && (
        <div className="mc-empty">
          <p className="mc-empty-title">No workspaces yet</p>
          <p>Add your first workspace to start queuing specs.</p>
          <pre className="mc-inline-cmd">
            zombiectl workspace add https://github.com/your-org/your-repo
          </pre>
        </div>
      )}

      {workspaces && workspaces.length > 0 && (
        <div className="workspace-grid">
          {workspaces.map((ws) => (
            <WorkspaceCard key={ws.id} workspace={ws} />
          ))}
        </div>
      )}

      <style>{`
        .mc-btn-primary {
          display: inline-flex;
          align-items: center;
          gap: 0.4rem;
          padding: 0.5rem 1rem;
          border-radius: var(--z-radius-pill);
          background: linear-gradient(120deg, var(--z-orange), var(--z-orange-bright));
          color: #111;
          font-size: 0.85rem;
          font-weight: 600;
          text-decoration: none;
          transition: box-shadow 0.2s;
        }
        .mc-btn-primary:hover {
          box-shadow: 0 0 20px var(--z-glow-strong);
        }
        .mc-error-banner {
          padding: 0.75rem 1rem;
          border-radius: var(--z-radius-md);
          border: 1px solid rgba(255, 77, 106, 0.3);
          background: rgba(255, 77, 106, 0.08);
          color: var(--z-red);
          font-size: 0.875rem;
          margin-bottom: 1.5rem;
        }
        .workspace-grid {
          display: grid;
          grid-template-columns: repeat(auto-fill, minmax(320px, 1fr));
          gap: 1rem;
        }
        .mc-empty-title {
          font-size: 1rem;
          font-weight: 600;
          color: var(--z-text-primary);
        }
        .mc-inline-cmd {
          display: inline-block;
          margin-top: 1rem;
          padding: 0.5rem 1rem;
          background: var(--z-surface-0);
          border: 1px solid var(--z-border);
          border-radius: var(--z-radius-md);
          font-family: var(--z-font-mono);
          font-size: 0.82rem;
          color: var(--z-cyan);
        }
      `}</style>
    </div>
  );
}
