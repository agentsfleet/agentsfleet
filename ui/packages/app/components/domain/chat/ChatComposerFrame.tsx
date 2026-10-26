"use client";

import type { ComponentProps, ReactNode, RefObject } from "react";
import { useEffect, useRef } from "react";
import { ComposerPrimitive } from "@assistant-ui/react";
import { DashboardPanel, SelectTrigger, Textarea } from "@agentsfleet/design-system";

type ModelProps = { children: ReactNode; select?: false } | ({ select: true } & Omit<ComponentProps<typeof SelectTrigger>, "appearance" | "className">);

/** Callers choose whether the model is editable; typography belongs here. */
export function ChatComposerModel({ children, select, ...trigger }: ModelProps) {
  return select ? <SelectTrigger {...trigger} appearance="composer" data-composer-part="model">{children}</SelectTrigger>
    : <span data-composer-part="model" className="min-w-0 truncate font-sans text-eyebrow leading-label text-muted-foreground">{children}</span>;
}

export function ChatComposerHint({ children }: { children: ReactNode }) {
  return <p data-composer-part="hint" className="mt-sm text-center font-sans text-eyebrow leading-label text-muted-foreground">{children}</p>;
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
  return <DashboardPanel asChild padding="none" className="min-h-0 rounded-xl bg-card px-3.5 py-lg focus-within:border-border-strong">
    <ComposerPrimitive.Root id={id} className="flex min-h-0 flex-col gap-sm" aria-label={label}>
      {children}
      <div className="flex min-h-chat-input flex-row items-stretch">
        <ComposerPrimitive.Input asChild placeholder={placeholder} submitMode="enter" disabled={disabled}>
          <Textarea appearance="composer" ref={inputRef} aria-label={placeholder} rows={2}
            className="field-sizing-content min-h-chat-input max-h-[min(12rem,30dvh)] flex-1 resize-none overflow-y-auto p-0 pointer-coarse:py-md" />
        </ComposerPrimitive.Input>
      </div>
      <div className="flex min-w-0 items-center justify-between gap-sm">{toolbar}{actions}</div>
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
