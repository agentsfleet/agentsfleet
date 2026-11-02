import type { ReactNode } from "react";
import { CircleCheckIcon, CircleAlertIcon, CircleHelpIcon, FileTextIcon, GitPullRequestIcon, GlobeIcon, ListIcon, PencilIcon, SquareTerminalIcon, WrenchIcon, XIcon } from "lucide-react";
import { SignalSpinner } from "@agentsfleet/design-system";
import { CELL_STATE, type CellState } from "./tool-call-copy";

export const TRIGGER_CLASS = "min-h-11 justify-start gap-sm rounded-sm px-md py-md text-body-sm text-muted-foreground hover:no-underline focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-info sm:min-h-9";
export const CELL_COPY: Record<CellState, {label: string; tone: string; icon: ReactNode}> = {
  [CELL_STATE.RUNNING]: {label: "Running", tone: "text-info", icon: <SignalSpinner className="size-3.5 text-current" aria-hidden="true" />},
  [CELL_STATE.SUCCEEDED]: {label: "Success", tone: "text-success", icon: <CircleCheckIcon className="size-3.5" aria-hidden="true" />},
  [CELL_STATE.FAILED]: {label: "Failed", tone: "text-destructive", icon: <XIcon className="size-3.5" aria-hidden="true" />},
  [CELL_STATE.INTERRUPTED]: {label: "Interrupted", tone: "text-warning", icon: <CircleAlertIcon className="size-3.5" aria-hidden="true" />},
  [CELL_STATE.SETTLED]: {label: "Outcome unavailable", tone: "text-text-dim", icon: <CircleHelpIcon className="size-3.5" aria-hidden="true" />},
};

export const ACTIVITY_ICONS = {read: FileTextIcon, write: PencilIcon, list: ListIcon, command: SquareTerminalIcon,
  request: GlobeIcon, "pull-request": GitPullRequestIcon, check: CircleCheckIcon, tool: WrenchIcon};
