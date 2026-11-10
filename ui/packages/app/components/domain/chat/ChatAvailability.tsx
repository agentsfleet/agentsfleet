import type {ReactNode} from "react";
import {AlertTitle, DashboardPanel, DashboardPanelDescription} from "@agentsfleet/design-system";
import {CircleAlertIcon} from "lucide-react";

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
