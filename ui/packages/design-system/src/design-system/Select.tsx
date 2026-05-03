"use client";

import * as SelectPrimitive from "@radix-ui/react-select";
import { type ComponentProps } from "react";
import { cn } from "../utils";

/*
 * Select — Radix Select composition with semantic utilities. Mirrors
 * shadcn/ui's compound API (Trigger / Content / Item / Value) so the
 * dashboard's existing select-shaped intent (ByokFields credential
 * picker, future filter dropdowns) stops re-implementing the Tailwind
 * styling for native <select> on every site.
 *
 * The chevron / check icons are inlined as SVGs so the design-system
 * package stays free of an icon-library transitive dep (see Dialog).
 */

function ChevronDownIcon() {
  return (
    <svg
      aria-hidden="true"
      width="14"
      height="14"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d="m6 9 6 6 6-6" />
    </svg>
  );
}

function CheckIcon() {
  return (
    <svg
      aria-hidden="true"
      width="14"
      height="14"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d="M20 6 9 17l-5-5" />
    </svg>
  );
}

export const Select = SelectPrimitive.Root;
export const SelectGroup = SelectPrimitive.Group;
export const SelectValue = SelectPrimitive.Value;

export type SelectTriggerProps = ComponentProps<typeof SelectPrimitive.Trigger>;

export function SelectTrigger({
  className,
  children,
  ref,
  ...props
}: SelectTriggerProps) {
  return (
    <SelectPrimitive.Trigger
      ref={ref}
      className={cn(
        "flex w-full items-center justify-between rounded-md border border-border bg-background",
        "px-3 py-2 text-sm shadow-sm transition-colors duration-200 ease-out",
        "focus:border-ring focus:outline-none focus:ring-1 focus:ring-ring",
        "disabled:cursor-not-allowed disabled:opacity-50",
        "data-[placeholder]:text-muted-foreground",
        className,
      )}
      {...props}
    >
      {children}
      <SelectPrimitive.Icon asChild>
        <span className="ml-2 shrink-0 opacity-60">
          <ChevronDownIcon />
        </span>
      </SelectPrimitive.Icon>
    </SelectPrimitive.Trigger>
  );
}

export type SelectContentProps = ComponentProps<typeof SelectPrimitive.Content>;

export function SelectContent({
  className,
  children,
  position = "popper",
  ref,
  ...props
}: SelectContentProps) {
  return (
    <SelectPrimitive.Portal>
      <SelectPrimitive.Content
        ref={ref}
        position={position}
        className={cn(
          "relative z-50 min-w-[8rem] overflow-hidden rounded-md border border-border",
          "bg-card text-card-foreground shadow-md",
          "data-[state=open]:animate-in data-[state=closed]:animate-out",
          "data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0",
          "data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95",
          position === "popper" &&
            "data-[side=bottom]:translate-y-1 data-[side=top]:-translate-y-1",
          className,
        )}
        {...props}
      >
        <SelectPrimitive.Viewport className="p-1">
          {children}
        </SelectPrimitive.Viewport>
      </SelectPrimitive.Content>
    </SelectPrimitive.Portal>
  );
}

export type SelectItemProps = ComponentProps<typeof SelectPrimitive.Item>;

export function SelectItem({ className, children, ref, ...props }: SelectItemProps) {
  return (
    <SelectPrimitive.Item
      ref={ref}
      className={cn(
        "relative flex w-full cursor-default select-none items-center rounded-sm",
        "py-1.5 pl-8 pr-2 text-sm outline-none transition-colors",
        "focus:bg-muted focus:text-foreground",
        "data-[disabled]:pointer-events-none data-[disabled]:opacity-50",
        className,
      )}
      {...props}
    >
      <span className="absolute left-2 flex size-4 items-center justify-center">
        <SelectPrimitive.ItemIndicator>
          <CheckIcon />
        </SelectPrimitive.ItemIndicator>
      </span>
      <SelectPrimitive.ItemText>{children}</SelectPrimitive.ItemText>
    </SelectPrimitive.Item>
  );
}

export type SelectLabelProps = ComponentProps<typeof SelectPrimitive.Label>;

export function SelectLabel({ className, ref, ...props }: SelectLabelProps) {
  return (
    <SelectPrimitive.Label
      ref={ref}
      className={cn("px-2 py-1.5 text-xs font-medium text-muted-foreground", className)}
      {...props}
    />
  );
}

export type SelectSeparatorProps = ComponentProps<typeof SelectPrimitive.Separator>;

export function SelectSeparator({ className, ref, ...props }: SelectSeparatorProps) {
  return (
    <SelectPrimitive.Separator
      ref={ref}
      className={cn("-mx-1 my-1 h-px bg-border", className)}
      {...props}
    />
  );
}
