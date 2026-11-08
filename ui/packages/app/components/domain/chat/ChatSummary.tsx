"use client";

import { createContext, useContext, useState, useRef, type ReactNode, type RefObject } from "react";
import { PanelRightIcon } from "lucide-react";
import { Button } from "@agentsfleet/design-system";

type SummaryState = {trigger: RefObject<HTMLButtonElement | null>; open: boolean; setOpen: (open: boolean) => void};
const SummaryContext = createContext<SummaryState | null>(null);
export const useChatSummary = () => useContext(SummaryContext);

export function ChatSummaryProvider({children}: {children: ReactNode}) {
  const trigger = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);
  return <SummaryContext value={{open, setOpen, trigger}}>{children}</SummaryContext>;
}

export function ChatSummaryToggle() {
  const summary = useChatSummary();
  if (!summary) return null;
  return <Button ref={summary.trigger} variant="ghost" size="icon" aria-label="Toggle fleet summary" title="Fleet summary" aria-expanded={summary.open} aria-controls="fleet-summary" onClick={() => summary.setOpen(!summary.open)}>
    <PanelRightIcon size={16} aria-hidden="true" />
  </Button>;
}
