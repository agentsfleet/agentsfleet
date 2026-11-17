import { memo } from "react";
import { ActionBarPrimitive } from "@assistant-ui/react";
import { CheckIcon, CopyIcon } from "@phosphor-icons/react/ssr";
import { Button } from "@agentsfleet/design-system";
import { QuoteReply } from "../../shared/chat/ChatQuote";

const COPY_REPLY_LABEL = "Copy reply";
export const REPLY_ACTIONS_TEST_ID = "fleet-reply-actions";

/**
 * The bar holds its height from the reply's first frame, so settling fills the
 * slot instead of pushing the reply up by it. Until the answer is final the
 * slot is empty: nothing to focus, nothing to announce.
 */
export const FleetReplyActions = memo(function FleetReplyActions({
  answer,
  settled,
}: {
  answer: string;
  settled: boolean;
}) {
  const controls = settled && answer.length > 0;
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
    </ActionBarPrimitive.Root>
  );
});
