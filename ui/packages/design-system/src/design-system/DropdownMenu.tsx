"use client";

import * as DropdownMenuPrimitive from "@radix-ui/react-dropdown-menu";
import { cva, type VariantProps } from "class-variance-authority";
import { type ComponentProps, type HTMLAttributes } from "react";
import { cn } from "../utils";
import { EYEBROW_CLASS } from "./eyebrow";

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

export type DropdownMenuContentProps = ComponentProps<typeof DropdownMenuPrimitive.Content> & {
  portalContainer?: HTMLElement | null;
};

export function DropdownMenuContent({
  className,
  sideOffset = 4,
  portalContainer,
  ref,
  ...props
}: DropdownMenuContentProps) {
  return (
    <DropdownMenuPrimitive.Portal container={portalContainer}>
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

// `eyebrow` heads a section in capitals ("WORKSPACE"). `name` carries a proper
// noun ("John's account") in sentence case at the label size, where capitals
// would shout it. Each variant owns its whole typography, so neither depends on
// the stylesheet's rule order to beat the other.
const dropdownMenuLabelVariants = cva("px-2.5 py-1.5 text-muted-foreground", {
  variants: {
    variant: {
      eyebrow: EYEBROW_CLASS,
      name: "font-sans text-label leading-label tracking-label",
    },
  },
  defaultVariants: { variant: "eyebrow" },
});

export type DropdownMenuLabelProps = ComponentProps<typeof DropdownMenuPrimitive.Label> &
  VariantProps<typeof dropdownMenuLabelVariants> & {
    inset?: boolean;
  };

export function DropdownMenuLabel({ className, inset, variant, ref, ...props }: DropdownMenuLabelProps) {
  return (
    <DropdownMenuPrimitive.Label
      ref={ref}
      className={cn(dropdownMenuLabelVariants({ variant }), inset && "pl-8", className)}
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
