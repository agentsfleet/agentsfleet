import { type ComponentProps } from "react";
import { cn } from "../utils";

/*
 * PageTitle — standard dashboard h1 with interface typography. RSC-safe.
 */
type PageTitleProps = ComponentProps<"h1">;

export function PageTitle({ className, ref, ...props }: PageTitleProps) {
  return (
    <h1
      ref={ref}
      className={cn(
        "font-sans text-display-md font-semibold leading-display-md tracking-display-md text-foreground",
        className,
      )}
      {...props}
    />
  );
}

export type { PageTitleProps };
export default PageTitle;
