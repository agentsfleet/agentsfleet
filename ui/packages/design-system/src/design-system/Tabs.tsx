"use client";

import * as TabsPrimitive from "@radix-ui/react-tabs";
import { type ComponentProps } from "react";
import { cn } from "../utils";

/*
 * Tabs — Radix Tabs composition. Client boundary because Radix uses DOM
 * APIs for arrow-key navigation, focus management, and the active-state
 * indicator. React 19 ref-as-prop.
 *
 * Compose: <Tabs defaultValue="..."><TabsList><TabsTrigger value=...>
 * Label</TabsTrigger></TabsList><TabsContent value=...>...</TabsContent>
 * </Tabs>
 */

export const Tabs = TabsPrimitive.Root;

export type TabsListProps = ComponentProps<typeof TabsPrimitive.List>;

export function TabsList({ className, ref, ...props }: TabsListProps) {
  return (
    <TabsPrimitive.List
      ref={ref}
      className={cn(
        "inline-flex h-10 items-center justify-start gap-1 rounded-lg bg-muted p-1",
        "text-muted-foreground",
        className,
      )}
      {...props}
    />
  );
}

export type TabsTriggerProps = ComponentProps<typeof TabsPrimitive.Trigger>;

export function TabsTrigger({ className, ref, ...props }: TabsTriggerProps) {
  return (
    <TabsPrimitive.Trigger
      ref={ref}
      className={cn(
        "inline-flex items-center justify-center whitespace-nowrap rounded-md px-3 py-1.5 text-sm font-medium",
        "ring-offset-background transition-all",
        "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring focus-visible:ring-offset-1",
        "disabled:pointer-events-none disabled:opacity-50",
        "data-[state=active]:bg-background data-[state=active]:text-foreground data-[state=active]:shadow-sm",
        className,
      )}
      {...props}
    />
  );
}

export type TabsContentProps = ComponentProps<typeof TabsPrimitive.Content>;

export function TabsContent({ className, ref, ...props }: TabsContentProps) {
  return (
    <TabsPrimitive.Content
      ref={ref}
      className={cn(
        "mt-2 ring-offset-background",
        "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring focus-visible:ring-offset-1",
        className,
      )}
      {...props}
    />
  );
}
