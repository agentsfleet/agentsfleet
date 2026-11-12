import { memo } from "react";
import { ActionBarPrimitive } from "@assistant-ui/react";
import { CheckIcon, CopyIcon } from "@phosphor-icons/react/ssr";
import { Button } from "@agentsfleet/design-system";
import { FleetReplyFigures } from "./FleetReplyFigures";
import type { ReplyFigures } from "./fleetMessageReaders";

const COPY_REPLY_LABEL = "Copy reply";

/** Completed replies expose native copy controls without reserving empty space while streaming. */
export const FleetReplyActions = memo(function FleetReplyActions({
  answer,
  settled,
  figures,
  activity,
}: {
  answer: string;
  settled: boolean;
  figures: ReplyFigures;
  activity?: unknown;
}) {
  if (!settled) return null;
  return (
    <ActionBarPrimitive.Root autohide="never" className="-ml-sm flex h-11 items-center gap-xs">
        {answer.length > 0 ? <ActionBarPrimitive.Copy asChild><Button variant="ghost" size="icon-sm"
          aria-label={COPY_REPLY_LABEL} title={COPY_REPLY_LABEL} className="group/copy">
          <CopyIcon aria-hidden="true" className="group-data-[copied]/copy:hidden" />
          <CheckIcon aria-hidden="true" className="hidden text-success group-data-[copied]/copy:block" />
        </Button></ActionBarPrimitive.Copy> : null}
        <FleetReplyFigures figures={figures} activity={activity} />
    </ActionBarPrimitive.Root>
  );
});
