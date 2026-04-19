"use client";

import * as DropdownMenuPrimitive from "@radix-ui/react-dropdown-menu";
import { type ComponentProps, type HTMLAttributes } from "react";
import { cn } from "../utils";

/*
 * DropdownMenu — Radix dropdown composition with semantic utilities.
 * Client boundary (portal + keyboard nav). React 19 ref-as-prop.
 */

export const DropdownMenu = DropdownMenuPrimitive.Root;
export const DropdownMenuTrigger = DropdownMenuPrimitive.Trigger;
export const DropdownMenuGroup = DropdownMenuPrimitive.Group;
export const DropdownMenuPortal = DropdownMenuPrimitive.Portal;
export const DropdownMenuSub = DropdownMenuPrimitive.Sub;
export const DropdownMenuRadioGroup = DropdownMenuPrimitive.RadioGroup;

export type DropdownMenuContentProps = ComponentProps<typeof DropdownMenuPrimitive.Content>;

export function DropdownMenuContent({
  className,
  sideOffset = 4,
  ref,
  ...props
}: DropdownMenuContentProps) {
  return (
    <DropdownMenuPrimitive.Portal>
      <DropdownMenuPrimitive.Content
        ref={ref}
        sideOffset={sideOffset}
        className={cn(
          "z-50 min-w-[10rem] overflow-hidden rounded-lg border border-border bg-popover p-1 shadow-xl",
          "data-[state=open]:animate-in data-[state=closed]:animate-out",
          "data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0",
          "data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95",
          className,
        )}
        {...props}
      />
    </DropdownMenuPrimitive.Portal>
  );
}

export type DropdownMenuItemProps = ComponentProps<typeof DropdownMenuPrimitive.Item> & {
  inset?: boolean;
};

export function DropdownMenuItem({ className, inset, ref, ...props }: DropdownMenuItemProps) {
  return (
    <DropdownMenuPrimitive.Item
      ref={ref}
      className={cn(
        "relative flex cursor-pointer select-none items-center gap-2 rounded-md px-2.5 py-1.5 text-sm outline-none transition-colors",
        "text-muted-foreground hover:bg-accent hover:text-foreground",
        "data-[disabled]:pointer-events-none data-[disabled]:opacity-50",
        "focus:bg-accent focus:text-foreground",
        inset && "pl-8",
        className,
      )}
      {...props}
    />
  );
}

export type DropdownMenuLabelProps = ComponentProps<typeof DropdownMenuPrimitive.Label> & {
  inset?: boolean;
};

export function DropdownMenuLabel({ className, inset, ref, ...props }: DropdownMenuLabelProps) {
  return (
    <DropdownMenuPrimitive.Label
      ref={ref}
      className={cn(
        "px-2.5 py-1.5 font-mono text-[0.65rem] uppercase tracking-widest text-muted-foreground",
        inset && "pl-8",
        className,
      )}
      {...props}
    />
  );
}

export type DropdownMenuSeparatorProps = ComponentProps<typeof DropdownMenuPrimitive.Separator>;

export function DropdownMenuSeparator({ className, ref, ...props }: DropdownMenuSeparatorProps) {
  return (
    <DropdownMenuPrimitive.Separator
      ref={ref}
      className={cn("-mx-1 my-1 h-px bg-border", className)}
      {...props}
    />
  );
}

export function DropdownMenuShortcut({
  className,
  ...props
}: HTMLAttributes<HTMLSpanElement>) {
  return (
    <span
      className={cn("ml-auto font-mono text-[0.7rem] tracking-widest text-muted-foreground", className)}
      {...props}
    />
  );
}
