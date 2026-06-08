"use client";

import { useId, type ReactNode } from "react";
import { Label, RadioGroupItem } from "@usezombie/design-system";
import type { ProviderMode } from "@/lib/types";

export type ModeRadioProps = {
  value: ProviderMode;
  checked: boolean;
  label: string;
  description: string;
  meta?: string;
  children?: ReactNode;
};

export default function ModeRadio({
  value,
  checked,
  label,
  description,
  meta,
  children,
}: ModeRadioProps) {
  const id = useId();
  return (
    <div
      data-active={checked}
      className="overflow-hidden rounded-md border border-border bg-card transition-colors duration-200 ease-out hover:border-border-strong data-[active=true]:border-primary"
    >
      <Label htmlFor={id} className="flex cursor-pointer items-start gap-3 p-4">
        <RadioGroupItem id={id} value={value} className="mt-0.5" />
        <span className="min-w-0 flex-1 space-y-1">
          <span className="flex flex-wrap items-center gap-2">
            <span className="font-medium">{label}</span>
            {meta ? (
              <span className="rounded-sm border border-border bg-muted px-1.5 py-0.5 font-mono text-label uppercase tracking-wide text-muted-foreground">
                {meta}
              </span>
            ) : null}
          </span>
          <span className="block text-xs font-normal text-muted-foreground">{description}</span>
        </span>
      </Label>
      {checked && children ? (
        <div className="border-t border-border bg-muted/20 px-4 py-4 animate-in fade-in-0 slide-in-from-top-1 duration-200">
          {children}
        </div>
      ) : null}
    </div>
  );
}
