import { type ComponentProps } from "react";

import { cn } from "../utils";

export type DashboardShellHeaderProps = ComponentProps<"header">;

/** Fixed-height dashboard chrome with an overlaid divider that does not consume layout height. */
export function DashboardShellHeader({ className, ref, ...props }: DashboardShellHeaderProps) {
  return (
    <header
      ref={ref}
      className={cn(
        "relative col-span-full sticky top-0 z-40 flex items-center gap-4 px-4 md:px-6 bg-background/85 backdrop-blur",
        "after:pointer-events-none after:absolute after:inset-x-0 after:bottom-0 after:h-px after:bg-border",
        className,
      )}
      {...props}
    />
  );
}

export default DashboardShellHeader;
