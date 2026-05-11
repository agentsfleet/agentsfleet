"use client";

import { useId } from "react";
import { Label, RadioGroupItem } from "@usezombie/design-system";
import type { ProviderMode } from "@/lib/types";

export type ModeRadioProps = {
  value: ProviderMode;
  checked: boolean;
  onChange: () => void;
  label: string;
  description: string;
};

/**
 * Single radio card in the Mode picker. Pure presentation — no state, no
 * fetch — so the parent ProviderSelector can be tested for orchestration
 * concerns without mounting browser-only APIs. Wraps the design-system
 * `<RadioGroupItem>` (Radix-backed) so the parent `<RadioGroup>` gets
 * arrow-key navigation, roving tabindex, and `data-state="checked"` for
 * free.
 */
export default function ModeRadio({
  value,
  checked,
  onChange,
  label,
  description,
}: ModeRadioProps) {
  const id = useId();
  return (
    <Label
      htmlFor={id}
      data-active={checked}
      className="flex cursor-pointer items-start gap-3 rounded-md border border-border p-3 transition-colors duration-200 ease-out hover:bg-accent/40 data-[active=true]:border-primary data-[active=true]:bg-accent/30"
    >
      <RadioGroupItem id={id} value={value} onClick={onChange} className="mt-0.5" />
      <span className="space-y-0.5">
        <span className="block font-medium">{label}</span>
        <span className="block text-xs font-normal text-muted-foreground">{description}</span>
      </span>
    </Label>
  );
}
