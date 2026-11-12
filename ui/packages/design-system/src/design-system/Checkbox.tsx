"use client";

import * as CheckboxPrimitive from "@radix-ui/react-checkbox";
import { CheckIcon } from "@phosphor-icons/react/ssr";
import type { ComponentProps } from "react";
import { cn } from "../utils";

export function Checkbox({className, ...props}: ComponentProps<typeof CheckboxPrimitive.Root>) {
  return <CheckboxPrimitive.Root {...props} className={cn(
    "peer inline-flex size-4.5 shrink-0 items-center justify-center rounded border border-border bg-background text-primary-foreground",
    "data-[state=checked]:border-primary data-[state=checked]:bg-primary",
    "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-4 focus-visible:ring-offset-background",
    "disabled:cursor-not-allowed disabled:opacity-50", className,
  )}>
    <CheckboxPrimitive.Indicator><CheckIcon size={14} weight="bold" aria-hidden="true" /></CheckboxPrimitive.Indicator>
  </CheckboxPrimitive.Root>;
}
