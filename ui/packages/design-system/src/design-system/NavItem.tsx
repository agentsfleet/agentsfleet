"use client";

import { type ComponentProps } from "react";
import { Slot } from "@radix-ui/react-slot";
import { cn } from "../utils";

export type NavItemProps = ComponentProps<"a"> & {
  asChild?: boolean;
  active?: boolean;
};

/** A navigation destination; asChild preserves the consumer's router link. */
export function NavItem({ asChild = false, active = false, className, onFocus, ...props }: NavItemProps) {
  const Component = asChild ? Slot : "a";
  return (
    <Component
      {...props}
      onFocus={(event) => {
        onFocus?.(event);
        if (event.defaultPrevented) return;
        const navigation = event.currentTarget.closest("nav");
        if (!navigation || navigation.scrollWidth <= navigation.clientWidth) return;
        const item = event.currentTarget.getBoundingClientRect();
        const viewport = navigation.getBoundingClientRect();
        if (item.left < viewport.left) navigation.scrollLeft += item.left - viewport.left;
        else if (item.right > viewport.right) navigation.scrollLeft += item.right - viewport.right;
      }}
      aria-current={active ? "page" : undefined}
      data-active={active ? "true" : undefined}
      className={cn(
        "flex min-h-11 shrink-0 items-center gap-md rounded-r-md border-l-2 border-transparent px-md py-sm font-sans text-body-sm text-muted-foreground no-underline",
        "transition duration-snap ease-snap hover:bg-accent hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
        "data-[active=true]:border-pulse data-[active=true]:bg-pulse/10 data-[active=true]:font-medium data-[active=true]:text-pulse",
        className,
      )}
    />
  );
}
