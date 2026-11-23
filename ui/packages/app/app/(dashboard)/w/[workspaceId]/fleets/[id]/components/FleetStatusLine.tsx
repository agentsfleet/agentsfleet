import { WarningCircleIcon, CoinsIcon, HashIcon, TimerIcon } from "@phosphor-icons/react/ssr";
import { StatusLine, StatusLineItem } from "@agentsfleet/design-system";
import { AGENTSFLEET_STATUS } from "@/lib/api/fleets-types";
import {
  METRICS_COST_LABEL,
  METRICS_OUTCOME_LABEL,
  METRICS_STATUS_LABEL,
  METRICS_STRIP_LABEL,
  METRICS_TIME_LABEL,
  METRICS_TOKENS_LABEL,
  METRICS_UNAVAILABLE,
  METRICS_VALUE_UNKNOWN,
} from "./console-copy";

const ICON_SIZE = 12;
const FIGURES = [
  {Icon: HashIcon, label: METRICS_TOKENS_LABEL, tone: undefined},
  {Icon: CoinsIcon, label: METRICS_COST_LABEL, tone: "foreground"},
  {Icon: TimerIcon, label: METRICS_TIME_LABEL, tone: undefined},
] as const;

/** The fallback shown while the fleet's latest run details are unavailable. */
export default function FleetStatusLine({status}: {status: string}) {
  return (
    <StatusLine aria-label={METRICS_STRIP_LABEL}
      className="flex-wrap gap-x-md gap-y-xs overflow-visible divide-x-0 [&>span]:px-0">
      <StatusLineItem tone={status === AGENTSFLEET_STATUS.ACTIVE ? "pulse" : "neutral"} className="normal-case">
        <span className="size-2 shrink-0 rounded-full bg-current" aria-hidden="true" />
        <span className="sr-only">{METRICS_STATUS_LABEL} </span>
        {status === AGENTSFLEET_STATUS.ARCHIVED ? "Archived" : status}
      </StatusLineItem>
      <StatusLineItem tone="neutral" className="min-w-0 max-w-full">
        <WarningCircleIcon size={ICON_SIZE} aria-hidden="true" className="shrink-0" />
        <span className="sr-only">{METRICS_OUTCOME_LABEL} </span>
        <span className="min-w-0 truncate">{METRICS_UNAVAILABLE}</span>
      </StatusLineItem>
      {FIGURES.map(({Icon, label, tone}) => (
        <StatusLineItem key={label} tone={tone}>
          <Icon size={ICON_SIZE} aria-hidden="true" className="shrink-0" />
          <span className="sr-only">{label} </span>
          <span>{METRICS_VALUE_UNKNOWN}</span>
        </StatusLineItem>
      ))}
    </StatusLine>
  );
}
