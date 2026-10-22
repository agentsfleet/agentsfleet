"use client";

import type { ReactNode, RefObject } from "react";
import { useEffect, useRef } from "react";
import { ComposerPrimitive } from "@assistant-ui/react";
import { DashboardPanel, Textarea } from "@agentsfleet/design-system";

type Props = {
  id?: string; label: string; placeholder: string;
  inputRef: RefObject<HTMLTextAreaElement | null>;
  disabled?: boolean; focusKey?: number;
  children?: ReactNode; toolbar?: ReactNode; actions: ReactNode;
};

/** Both chat purposes use the same input, focus lifecycle and height limits. */
export function ChatComposerFrame({id, label, placeholder, inputRef, disabled = false, focusKey, children, toolbar, actions}: Props) {
  useComposerFocus(inputRef, disabled, focusKey);
  return <DashboardPanel asChild padding="none" className="min-h-0 rounded-xl bg-card p-md focus-within:border-border-strong">
    <ComposerPrimitive.Root id={id} className="flex min-h-0 flex-col gap-sm" aria-label={label}>
      {children}
      <div className="flex min-h-9 flex-row items-stretch gap-sm pointer-coarse:min-h-11 sm:gap-md">
        <ComposerPrimitive.Input asChild placeholder={placeholder} submitMode="enter" disabled={disabled}>
          <Textarea appearance="composer" ref={inputRef} aria-label={placeholder} rows={1}
            className="field-sizing-content min-h-9 max-h-[min(12rem,30dvh)] flex-1 resize-none overflow-y-auto px-sm py-xs text-reading leading-reading text-foreground placeholder:text-muted-foreground pointer-coarse:py-md" />
        </ComposerPrimitive.Input>
        {toolbar ? null : actions}
      </div>
      {toolbar ? <div className="flex min-w-0 items-end gap-sm">{toolbar}{actions}</div> : null}
    </ComposerPrimitive.Root>
  </DashboardPanel>;
}

function useComposerFocus(input: RefObject<HTMLTextAreaElement | null>, disabled: boolean, focusKey?: number): void {
  const previous = useRef<boolean | null>(null);
  const previousKey = useRef(focusKey);
  useEffect(() => {
    const initial = previous.current === null;
    const wasDisabled = previous.current;
    previous.current = disabled;
    const requested = previousKey.current !== focusKey;
    previousKey.current = focusKey;
    const field = input.current;
    if (!field || disabled) return;
    const active = field.ownerDocument.activeElement;
    const available = active === field.ownerDocument.body || field.closest("form")?.contains(active);
    if (initial || requested || (wasDisabled && available)) field.focus({preventScroll: true});
  }, [input, disabled, focusKey]);
}
