"use client";

import * as DropdownMenuPrimitive from "@radix-ui/react-dropdown-menu";
import { cva, type VariantProps } from "class-variance-authority";
import { type ComponentProps, type HTMLAttributes } from "react";
import { cn } from "../utils";
import { EYEBROW_CLASS } from "./eyebrow";
import { CaretRightIcon } from "@phosphor-icons/react/ssr";

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

const MENU_ITEM_CLASS = "relative flex min-h-10 cursor-pointer select-none items-center gap-2 rounded-control px-2.5 py-2 font-control text-body-sm leading-body-sm outline-none transition-colors [&_svg]:pointer-events-none [&_svg]:size-4 [&_svg]:shrink-0 text-foreground hover:bg-accent data-[disabled]:pointer-events-none data-[disabled]:opacity-50 focus:bg-accent focus:text-foreground pointer-coarse:min-h-11";
const MENU_CONTENT_CLASS = "z-50 min-w-[10rem] overflow-hidden rounded-md border border-border bg-popover p-1 shadow-floating data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95";

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
          MENU_CONTENT_CLASS,
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
        MENU_ITEM_CLASS,
        inset && "pl-8",
        className,
      )}
      {...props}
    />
  );
}

export function DropdownMenuSubTrigger({className, children, ref, ...props}: ComponentProps<typeof DropdownMenuPrimitive.SubTrigger>) {
  return <DropdownMenuPrimitive.SubTrigger ref={ref} className={cn(MENU_ITEM_CLASS, className)} {...props}>
    {children}<CaretRightIcon aria-hidden="true" className="ml-auto shrink-0" />
  </DropdownMenuPrimitive.SubTrigger>;
}

export function DropdownMenuSubContent({className, portalContainer, ...props}: ComponentProps<typeof DropdownMenuPrimitive.SubContent> & {portalContainer?: HTMLElement | null}) {
  return <DropdownMenuPrimitive.Portal container={portalContainer}>
    <DropdownMenuPrimitive.SubContent className={cn(MENU_CONTENT_CLASS, className)} {...props} />
  </DropdownMenuPrimitive.Portal>;
}

// Compact section labels and proper names keep sentence case at their own type scale.
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
