import { type ComponentProps } from "react";
import { cn } from "../utils";

/*
 * PageTitle — standard dashboard h1 with interface typography. RSC-safe.
 */
type PageTitleProps = ComponentProps<"h1"> & {size?: "default" | "chat"};

export function PageTitle({ className, size = "default", ref, ...props }: PageTitleProps) {
  return (
    <h1
      ref={ref}
      className={cn(
        "font-sans font-semibold text-foreground",
        size === "chat" ? "text-chat-title leading-heading tracking-tight" : "text-display-md leading-display-md tracking-display-md",
        className,
      )}
      {...props}
    />
  );
}

export type { PageTitleProps };
export default PageTitle;
