"use client";

import * as RadioGroupPrimitive from "@radix-ui/react-radio-group";
import { type ComponentProps, type ReactNode } from "react";
import { cn } from "../utils";

/*
 * OptionCard — a bordered "choice card" (icon + label + optional
 * description), built on the existing RadioGroupPrimitive.Item so it
 * inherits Radix's accessible radio semantics (role="radio", arrow-key
 * navigation, roving tab-index) instead of a second hand-rolled
 * implementation. Renders inside <RadioGroup>, same as RadioGroupItem.
 */
export interface OptionCardProps
  extends Omit<ComponentProps<typeof RadioGroupPrimitive.Item>, "children"> {
  label: string;
  description?: ReactNode;
  icon?: ReactNode;
}

export function OptionCard({ label, description, icon, className, ...rest }: OptionCardProps) {
  return (
    <RadioGroupPrimitive.Item
      className={cn(
        "flex w-full items-start gap-3 rounded-lg border border-border bg-card p-4 text-left transition-colors",
        "hover:border-primary/40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2",
        "data-[state=checked]:border-primary data-[state=checked]:ring-1 data-[state=checked]:ring-primary",
        "disabled:cursor-not-allowed disabled:opacity-50",
        className,
      )}
      {...rest}
    >
      {icon ? <span className="mt-0.5 text-muted-foreground">{icon}</span> : null}
      <span className="flex flex-col gap-1">
        <span className="text-sm font-medium text-foreground">{label}</span>
        {description ? (
          <span className="text-xs text-muted-foreground">{description}</span>
        ) : null}
      </span>
    </RadioGroupPrimitive.Item>
  );
}
