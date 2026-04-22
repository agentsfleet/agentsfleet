"use client";

import * as AccordionPrimitive from "@radix-ui/react-accordion";
import { type ComponentProps } from "react";
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
          "flex flex-1 items-center justify-between py-4 text-sm font-medium",
          "transition-all hover:underline text-left",
          "[&[data-state=open]>svg]:rotate-180",
          className,
        )}
        {...props}
      >
        {children}
        <svg
          xmlns="http://www.w3.org/2000/svg"
          width="16"
          height="16"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
          className="ml-2 h-4 w-4 shrink-0 text-muted-foreground transition-transform duration-200"
          aria-hidden="true"
        >
          <path d="m6 9 6 6 6-6" />
        </svg>
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
        "data-[state=closed]:animate-accordion-up data-[state=open]:animate-accordion-down",
      )}
      {...props}
    >
      <div className={cn("pb-4 pt-0", className)}>{children}</div>
    </AccordionPrimitive.Content>
  );
}
