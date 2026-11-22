"use client";

import { type ComponentProps } from "react";
import { Slot } from "@radix-ui/react-slot";
import { cn } from "../utils";

export type NavItemProps = ComponentProps<"a"> & {
  asChild?: boolean;
  active?: boolean;
  appearance?: "default" | "rail" | "list";
  tone?: "default" | "accent";
};

/**
 * A navigation destination; asChild preserves the consumer's router link.
 *
 * A pill, not a rail. The active state is the fill and the weight; an accent
 * rail on the left edge belongs flush against a container edge, and every
 * consumer insets its items, so the rail floated 12px in and read as a
 * clipped corner. Dropping it also puts the icon on the same column as the
 * group eyebrows, which the rail's two pixels had pushed it off.
 */
export function NavItem({ asChild = false, active = false, appearance = "default", tone = "default", className, onFocus, ...props }: NavItemProps) {
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
        "flex min-h-9 shrink-0 items-center gap-md rounded-md px-md py-xs font-sans text-body-sm text-muted-foreground no-underline pointer-coarse:min-h-11",
        "transition duration-snap ease-snap hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring data-[active=true]:font-medium",
        appearance === "default"
          ? "hover:bg-accent data-[active=true]:bg-pulse/10 data-[active=true]:text-pulse"
          : "w-full hover:bg-muted data-[active=true]:bg-muted data-[active=true]:text-foreground",
        appearance === "rail" && "min-h-11 gap-lg px-lg font-medium md:min-h-12",
        appearance === "list" && "text-label leading-label",
        tone === "accent" && "bg-primary/10 text-foreground hover:bg-primary/15 data-[active=true]:bg-primary/15 data-[active=true]:text-foreground",
        className,
      )}
    />
  );
}
