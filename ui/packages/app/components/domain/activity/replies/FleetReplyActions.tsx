import { memo } from "react";
import { ActionBarPrimitive } from "@assistant-ui/react";
import { CheckIcon, CopyIcon, ListBulletsIcon } from "@phosphor-icons/react/ssr";
import { Button, Popover, PopoverContent, PopoverTrigger } from "@agentsfleet/design-system";
import { readDurableTaskActivityHistory } from "@/lib/task-activity";
import { QuoteReply } from "../../shared/chat/ChatQuote";
import { DurableTaskActivityTimeline } from "@/components/domain/tasks/TaskActivityTimeline";

const COPY_REPLY_LABEL = "Copy reply";
const TASK_ACTIVITY_LABEL = "Task activity";
export const REPLY_ACTIONS_TEST_ID = "fleet-reply-actions";

/**
 * The bar holds its height from the reply's first frame, so settling fills the
 * slot instead of pushing the reply up by it. Until the answer is final the
 * slot is empty: nothing to focus, nothing to announce.
 */
export const FleetReplyActions = memo(function FleetReplyActions({
  answer,
  settled,
  activity,
}: {
  answer: string;
  settled: boolean;
  activity?: unknown;
}) {
  const controls = settled && answer.length > 0;
  const hasActivity = !!readDurableTaskActivityHistory(activity)?.items.length;
  return (
    <ActionBarPrimitive.Root autohide="never" data-testid={REPLY_ACTIONS_TEST_ID} className="-ml-sm flex h-11 items-center gap-xs">
      {controls ? <>
        <ActionBarPrimitive.Copy asChild><Button variant="ghost" size="icon-sm"
          aria-label={COPY_REPLY_LABEL} title={COPY_REPLY_LABEL} className="group/copy">
          <CopyIcon aria-hidden="true" className="group-data-[copied]/copy:hidden" />
          <CheckIcon aria-hidden="true" className="hidden text-success group-data-[copied]/copy:block" />
        </Button></ActionBarPrimitive.Copy>
        <QuoteReply text={answer} />
      </> : null}
      {hasActivity ? <Popover>
        <PopoverTrigger asChild>
          <Button variant="ghost" size="icon-sm" aria-label={TASK_ACTIVITY_LABEL} title={TASK_ACTIVITY_LABEL}>
            <ListBulletsIcon aria-hidden="true" />
          </Button>
        </PopoverTrigger>
        <PopoverContent side="top" align="start" aria-label={TASK_ACTIVITY_LABEL}>
          <DurableTaskActivityTimeline value={activity} running={false} />
        </PopoverContent>
      </Popover> : null}
    </ActionBarPrimitive.Root>
  );
});
