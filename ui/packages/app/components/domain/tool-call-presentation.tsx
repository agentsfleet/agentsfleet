import type { ReactNode } from "react";
import { ClockIcon, CheckCircleIcon, WarningCircleIcon, QuestionIcon, FileTextIcon, GitPullRequestIcon, GlobeIcon, ListIcon, PencilSimpleIcon, TerminalWindowIcon, WrenchIcon, XIcon } from "@phosphor-icons/react/ssr";
import { CELL_STATE, type CellState } from "./tool-call-copy";

export const TRIGGER_CLASS = "min-h-11 w-full min-w-0 justify-start gap-sm rounded-sm px-md py-md text-body-sm text-muted-foreground hover:no-underline focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-info sm:min-h-9";
export const CELL_COPY: Record<CellState, {label: string; tone: string; icon: ReactNode}> = {
  [CELL_STATE.RUNNING]: {label: "Running", tone: "text-info", icon: <ClockIcon className="size-4" aria-hidden="true" />},
  [CELL_STATE.SUCCEEDED]: {label: "Success", tone: "text-success", icon: <CheckCircleIcon className="size-4" aria-hidden="true" />},
  [CELL_STATE.FAILED]: {label: "Failed", tone: "text-destructive", icon: <XIcon className="size-4" aria-hidden="true" />},
  [CELL_STATE.INTERRUPTED]: {label: "Interrupted", tone: "text-warning", icon: <WarningCircleIcon className="size-4" aria-hidden="true" />},
  [CELL_STATE.SETTLED]: {label: "Outcome unavailable", tone: "text-text-dim", icon: <QuestionIcon className="size-4" aria-hidden="true" />},
};

export const ACTIVITY_ICONS = {read: FileTextIcon, write: PencilSimpleIcon, list: ListIcon, command: TerminalWindowIcon,
  request: GlobeIcon, "pull-request": GitPullRequestIcon, check: CheckCircleIcon, tool: WrenchIcon};
