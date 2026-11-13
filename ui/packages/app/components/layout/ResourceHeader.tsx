"use client";

import Link from "next/link";
import { useFreshFleetChat } from "./draft-sidebar-state";
import { workspaceIdFromPath, workspacePath } from "@/lib/workspace-routes";
import { usePathname } from "next/navigation";
import { CpuIcon, UsersIcon, ChartBarIcon, KeyIcon, NotePencilIcon } from "@phosphor-icons/react/ssr";
import { PageHeader, TooltipButton } from "@agentsfleet/design-system";
import { PRIMARY_NAV } from "./sidebar-navigation-data";

const ACCOUNT_PAGES = [
  {path: "settings/models", label: "Models", icon: CpuIcon},
  {path: "settings/members", label: "Team", icon: UsersIcon},
  {path: "settings/billing", label: "Usage", icon: ChartBarIcon},
  {path: "settings/security", label: "API Keys", icon: KeyIcon},
] as const;

/** Display only: routing, authorization and account selection keep their owners. */
export function ResourceHeader({workspaceId: selectedWorkspace = null}: {workspaceId?: string | null}) {
  const pathname = usePathname();
  const workspaceId = workspaceIdFromPath(pathname) ?? selectedWorkspace;
  const page = [...PRIMARY_NAV, ...ACCOUNT_PAGES].find(entry => entry.path !== "fleets/new" && pathname.endsWith(`/${entry.path}`));
  if (!page) return null;
  const Icon = page.icon;
  return <PageHeader data-resource-header className="h-16 shrink-0 flex-row items-center gap-lg border-b border-border-subtle px-lg summary-medium:px-xl summary-wide:px-2xl" actions={workspaceId ? <NewFleetShortcut workspaceId={workspaceId} /> : null}>
    <span className="flex items-center gap-md text-label summary-medium:gap-lg font-semibold"><Icon data-app-header-icon="" size={16} className="hidden sm:block" aria-hidden="true" /><span data-app-header-title="">{page.label}</span></span>
  </PageHeader>;
}

/** One navigation intent for account, resource and conversation headers. */
export function NewFleetShortcut({workspaceId}: {workspaceId: string}) {
  const fresh = useFreshFleetChat(workspaceId);
  return <TooltipButton tooltip="New fleet" asChild variant="ghost" size="icon" aria-label="New fleet"><Link href={workspacePath(workspaceId, "fleets/new")} onClick={fresh}><NotePencilIcon size={16} aria-hidden="true" /></Link></TooltipButton>;
}
