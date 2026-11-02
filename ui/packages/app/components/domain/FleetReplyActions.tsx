import { memo } from "react";
import { CopyButton } from "@agentsfleet/design-system";
import { FleetReplyFigures } from "./FleetReplyFigures";
import type { ReplyFigures } from "./fleetMessageReaders";

const COPY_REPLY_LABEL = "Copy reply";

/** Settled controls keep their reserved height while another reply streams. */
export const FleetReplyActions = memo(function FleetReplyActions({
  answer,
  settled,
  figures,
}: {
  answer: string;
  settled: boolean;
  figures: ReplyFigures;
}) {
  return (
    <div className="-ml-sm flex h-11 items-center gap-xs">
      {settled ? <>
        {answer.length > 0 ? <CopyButton value={answer} label={COPY_REPLY_LABEL} /> : null}
        <FleetReplyFigures figures={figures} />
      </> : null}
    </div>
  );
});
