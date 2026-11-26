import { type ComponentProps } from "react";
import { cn } from "../utils";

/*
 * PageTitle — standard dashboard h1 with interface typography. RSC-safe.
 */
type PageTitleProps = ComponentProps<"h1"> & {as?: "h1" | "h2"; size?: "default" | "chat"};

export function PageTitle({ as: Title = "h1", className, size = "default", ref, ...props }: PageTitleProps) {
  return (
    <Title
      ref={ref}
      className={cn(
        "font-sans font-title text-foreground",
        size === "chat" ? "text-chat-title leading-label tracking-label" : "text-display-md leading-display-md tracking-display-md",
        className,
      )}
      {...props}
    />
  );
}

export type { PageTitleProps };
