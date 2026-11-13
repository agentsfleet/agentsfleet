"use client";

import * as AccordionPrimitive from "@radix-ui/react-accordion";
import { type ComponentProps } from "react";
import { CaretDownIcon } from "@phosphor-icons/react/ssr";
import { cn } from "../utils";

/*
 * Accordion — Radix Accordion composition. Client boundary because Radix
 * uses DOM APIs for keyboard navigation (Up/Down/Home/End), focus, and
 * the open/close animation. React 19 ref-as-prop.
 *
 * Usage:
 *   <Accordion type="single" collapsible>
 *     <AccordionItem value="q1">
 *       <AccordionTrigger>Question?</AccordionTrigger>
 *       <AccordionContent>Answer.</AccordionContent>
 *     </AccordionItem>
 *   </Accordion>
 */

export const Accordion = AccordionPrimitive.Root;

export type AccordionItemProps = ComponentProps<typeof AccordionPrimitive.Item>;

export function AccordionItem({ className, ref, ...props }: AccordionItemProps) {
  return (
    <AccordionPrimitive.Item
      ref={ref}
      className={cn("border-b border-border", className)}
      {...props}
    />
  );
}

export type AccordionTriggerProps = ComponentProps<typeof AccordionPrimitive.Trigger>;

export function AccordionTrigger({ className, children, ref, ...props }: AccordionTriggerProps) {
  return (
    <AccordionPrimitive.Header className="flex">
      <AccordionPrimitive.Trigger
        ref={ref}
        className={cn(
          "flex flex-1 items-center justify-between rounded-control py-4 text-sm font-medium",
          "transition-all hover:text-foreground text-left motion-reduce:transition-none",
          "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-4 focus-visible:ring-offset-background",
          "[&[data-state=open]>svg]:rotate-180",
          className,
        )}
        {...props}
      >
        {children}
        <CaretDownIcon size={16} className="ml-2 shrink-0 text-muted-foreground transition-transform duration-200 motion-reduce:transition-none" aria-hidden="true" />
      </AccordionPrimitive.Trigger>
    </AccordionPrimitive.Header>
  );
}

export type AccordionContentProps = ComponentProps<typeof AccordionPrimitive.Content>;

export function AccordionContent({ className, children, ref, ...props }: AccordionContentProps) {
  return (
    <AccordionPrimitive.Content
      ref={ref}
      className={cn(
        "overflow-hidden text-sm",
        "data-[state=closed]:animate-accordion-up data-[state=open]:animate-accordion-down motion-reduce:animate-none",
      )}
      {...props}
    >
      <div className={cn("pb-4 pt-0", className)}>{children}</div>
    </AccordionPrimitive.Content>
  );
}
