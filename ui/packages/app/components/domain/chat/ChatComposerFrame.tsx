"use client";

import type { ComponentProps, ReactNode, RefObject } from "react";
import { useEffect, useRef } from "react";
import { ComposerPrimitive } from "@assistant-ui/react";
import { DashboardPanel, SelectTrigger, Textarea } from "@agentsfleet/design-system";
import { CpuIcon } from "@phosphor-icons/react/ssr";

type ModelProps = { children: ReactNode; select?: false } | ({ select: true } & Omit<ComponentProps<typeof SelectTrigger>, "appearance" | "className">);

/** Callers choose whether the model is editable; typography belongs here. */
export function ChatComposerModel({ children, select, ...trigger }: ModelProps) {
  const content = <span className="inline-flex min-w-0 items-center gap-md"><CpuIcon size={16} className="shrink-0" aria-hidden="true" /><span className="min-w-0 truncate">{children}</span></span>;
  return select ? <SelectTrigger {...trigger} appearance="composer" data-composer-part="model">{content}</SelectTrigger>
    : <span data-composer-part="model" className="min-w-0 font-sans text-eyebrow leading-label text-muted-foreground">{content}</span>;
}

export function ChatComposerHint({ children }: { children?: ReactNode }) {
  return <div data-composer-part="hint" className="mt-sm flex items-center justify-between gap-lg px-sm font-sans text-eyebrow leading-eyebrow text-muted-foreground">
    <span className="min-w-0">Enter to send · Shift + Enter for a new line</span>
    {children ? <span className="min-w-0">{children}</span> : null}
  </div>;
}

type Props = {
  id?: string; label: string; placeholder: string;
  inputRef: RefObject<HTMLTextAreaElement | null>;
  disabled: boolean; focusKey?: number;
  children?: ReactNode; toolbar: ReactNode; actions: ReactNode;
};

/** Both chat purposes use the same input, focus lifecycle and height limits. */
export function ChatComposerFrame({id, label, placeholder, inputRef, disabled, focusKey, children, toolbar, actions}: Props) {
  useComposerFocus(inputRef, disabled, focusKey);
  return <DashboardPanel asChild padding="none" className="min-h-0 shrink-0 rounded-lg bg-card px-xl pb-lg pt-xl transition-colors duration-150 focus-within:border-border-strong motion-reduce:transition-none">
    <ComposerPrimitive.Root id={id} className="flex min-h-0 flex-col gap-sm" aria-label={label}>
      {children}
      <div className="flex min-h-chat-input flex-row items-stretch">
        <ComposerPrimitive.Input asChild placeholder={placeholder} submitMode="enter" disabled={disabled}>
          <Textarea appearance="composer" ref={inputRef} aria-label={placeholder} rows={2}
            className="field-sizing-content min-h-chat-input max-h-[min(12rem,30dvh)] flex-1 resize-none overflow-y-auto p-0 pointer-coarse:py-md" />
        </ComposerPrimitive.Input>
      </div>
      <div className="mt-md flex min-w-0 items-center justify-between gap-lg"><div className="flex min-w-0 flex-1 items-center gap-sm">{toolbar}</div><div className="flex shrink-0 items-center gap-sm">{actions}</div></div>
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
    const overlay = active?.closest('[role="menu"], [role="dialog"]');
    const initialAvailable = !overlay || overlay.contains(field);
    if ((initial && initialAvailable) || requested || (wasDisabled && available)) field.focus({preventScroll: true});
  }, [input, disabled, focusKey]);
}
