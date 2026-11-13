"use client";

import * as RadioGroupPrimitive from "@radix-ui/react-radio-group";
import type { ComponentProps } from "react";
import { cn } from "../utils";

export const RadioGroup = RadioGroupPrimitive.Root;

export function RadioGroupItem({className, ...props}: ComponentProps<typeof RadioGroupPrimitive.Item>) {
  return <RadioGroupPrimitive.Item {...props} className={cn(
    "inline-flex size-4 shrink-0 items-center justify-center rounded-full border border-border bg-background text-primary data-[state=checked]:border-primary",
    "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-4 focus-visible:ring-offset-background",
    "disabled:cursor-not-allowed disabled:opacity-50", className,
  )}>
    <RadioGroupPrimitive.Indicator className="size-2 rounded-full bg-current" />
  </RadioGroupPrimitive.Item>;
}
