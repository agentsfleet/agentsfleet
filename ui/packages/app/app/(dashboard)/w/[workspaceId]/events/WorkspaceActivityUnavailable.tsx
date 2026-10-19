import Link from "next/link";
import { Button, UnavailableState } from "@agentsfleet/design-system";
import { ApiError } from "@/lib/api/errors";
import { RouteRetryButton } from "@/components/domain/RouteRetryButton";
import { workspacePath } from "@/lib/workspace-routes";

export function WorkspaceActivityUnavailable({error, workspaceId, hasCursor}: {error: unknown; workspaceId: string; hasCursor: boolean}) {
  if (error instanceof ApiError && error.status === 401) return <UnavailableState title="Sign in to see activity"
    description="Your session has expired. Sign in again to continue."
    actions={<Button asChild><Link href="/sign-in">Sign in</Link></Button>} />;
  if (error instanceof ApiError && (error.status === 403 || error.status === 404)) return <UnavailableState title="Activity isn’t available in this workspace"
    description="You may need access from the workspace owner. You can choose another workspace."
    actions={<Button asChild variant="secondary"><Link href="/">Choose workspace</Link></Button>} />;
  if (error instanceof ApiError && error.status === 400 && hasCursor) return <UnavailableState title="This activity page has expired"
    description="Open the latest activity to continue browsing."
    actions={<Button asChild variant="secondary"><Link href={workspacePath(workspaceId, "events")}>Latest activity</Link></Button>} />;
  return <UnavailableState title="Activity couldn’t be loaded"
    description="Retry to load activity in this workspace."
    actions={<RouteRetryButton label="Retry" />} />;
}
