"use client";

import * as PopoverPrimitive from "@radix-ui/react-popover";
import type { ComponentProps } from "react";
import { cn } from "../utils";

export const Popover = PopoverPrimitive.Root;
export const PopoverTrigger = PopoverPrimitive.Trigger;
export type PopoverContentProps = ComponentProps<typeof PopoverPrimitive.Content>;

/** Contextual details beside their trigger, with focus and dismissal from Radix. */
export function PopoverContent({ className, sideOffset = 4, collisionPadding = 8, ref, ...props }: PopoverContentProps) {
  return <PopoverPrimitive.Portal>
    <PopoverPrimitive.Content ref={ref} sideOffset={sideOffset} collisionPadding={collisionPadding}
      className={cn("z-50 w-64 max-w-[calc(100vw-var(--spacing-xl))] rounded-md border border-border-strong bg-popover p-md font-sans text-body-sm text-foreground shadow-floating outline-none", className)}
      {...props} />
  </PopoverPrimitive.Portal>;
}
