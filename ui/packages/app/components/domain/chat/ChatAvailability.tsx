import type {ReactNode} from "react";
import Link from "next/link";
import {AlertTitle, Button, DashboardPanel, DashboardPanelDescription, Skeleton} from "@agentsfleet/design-system";
import {CircleAlertIcon} from "lucide-react";
import type {DraftClient} from "@/lib/api/drafts";
import type {DraftView} from "@/lib/api/draft-state";
import {DRAFT_LIST, DRAFT_LIST_STATUS} from "@/lib/api/draft-list-types";
import {DraftLoadMore, LOAD_MORE_COPY} from "./DraftLoadMore";

export function ChatAvailability({title, children, actions}: {title: string; children: ReactNode; actions: ReactNode}) {
  return <DashboardPanel role="alert" padding="compact" className="mx-auto flex w-full max-w-measure items-start gap-md">
    <CircleAlertIcon size={18} className="mt-xs shrink-0 text-warning" aria-hidden="true" />
    <div className="min-w-0 flex-1 space-y-sm">
      <AlertTitle>{title}</AlertTitle>
      <DashboardPanelDescription>{children}</DashboardPanelDescription>
      <div className="flex flex-wrap items-center gap-sm">{actions}</div>
    </div>
  </DashboardPanel>;
}

export function ChatModelAvailability({client, view, workspace, canManagePlatformModels = false}: {client: DraftClient; view: DraftView; workspace: string; canManagePlatformModels?: boolean}) {
  if (view.error || view.inaccessible || view.models.some(model => model.eligibility.authoring)) return null;
  if (view.listStatus === DRAFT_LIST_STATUS.loading) return <Skeleton className="mx-auto h-24 w-full max-w-measure" aria-label="Loading chat models" />;
  if (view.listStatus !== DRAFT_LIST_STATUS.ready) return null;
  const incomplete = view.moreModels;
  const configured = view.models.length > 0;
  const description = incomplete ? "More configured models are available to check. Load them to find a model for chat."
    : configured ? "Your configured models are not available for setup chat. Check their provider credentials and model configuration."
    : "Configure a workspace model or ask your administrator to configure a platform model to start chatting.";
  return <ChatAvailability title={incomplete ? "Load more chat models" : configured ? "No chat model is available" : "Create a model to start chatting"}
    actions={<>
      {incomplete ? <DraftLoadMore copy={LOAD_MORE_COPY.models} onLoad={() => client.loadMore(DRAFT_LIST.models)} /> : null}
      <Button size="sm" variant="secondary" asChild><Link href={canManagePlatformModels ? "/admin/models" : `/w/${workspace}/settings/models`}>Create model</Link></Button>
    </>}>{description}</ChatAvailability>;
}
