"use client";

import * as TooltipPrimitive from "@radix-ui/react-tooltip";
import { type ComponentProps } from "react";
import { cn } from "../utils";

/*
 * Tooltip — Radix Tooltip composition with semantic utilities. Client
 * boundary (portal + pointer tracking). React 19 ref-as-prop. Wrap the
 * app root (or a subtree) in <TooltipProvider> for delay coordination.
 */

const TOOLTIP_DELAY_DURATION_MS = 120;
const TOOLTIP_SKIP_DELAY_DURATION_MS = 80;

export type TooltipProviderProps = ComponentProps<typeof TooltipPrimitive.Provider>;

export function TooltipProvider({
  delayDuration = TOOLTIP_DELAY_DURATION_MS,
  skipDelayDuration = TOOLTIP_SKIP_DELAY_DURATION_MS,
  ...props
}: TooltipProviderProps) {
  return (
    <TooltipPrimitive.Provider
      delayDuration={delayDuration}
      skipDelayDuration={skipDelayDuration}
      {...props}
    />
  );
}

export const Tooltip = TooltipPrimitive.Root;
export const TooltipTrigger = TooltipPrimitive.Trigger;

export type TooltipContentProps = ComponentProps<typeof TooltipPrimitive.Content>;

export function TooltipContent({
  className,
  sideOffset = 4,
  ref,
  ...props
}: TooltipContentProps) {
  return (
    <TooltipPrimitive.Portal>
      <TooltipPrimitive.Content
        ref={ref}
        sideOffset={sideOffset}
        className={cn(
          "z-50 overflow-hidden rounded-md border border-border bg-popover px-3 py-1.5",
          "font-mono text-xs text-foreground shadow-md",
          "will-change-[transform,opacity]",
          "animate-in fade-in-0 zoom-in-95 duration-150 ease-out",
          "data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=closed]:zoom-out-95 data-[state=closed]:duration-100",
          "data-[side=bottom]:slide-in-from-top-1 data-[side=left]:slide-in-from-right-1",
          "data-[side=right]:slide-in-from-left-1 data-[side=top]:slide-in-from-bottom-1",
          className,
        )}
        {...props}
      />
    </TooltipPrimitive.Portal>
  );
}
