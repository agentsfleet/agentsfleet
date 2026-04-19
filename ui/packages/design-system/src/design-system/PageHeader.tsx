import { type ComponentProps } from "react";
import { cn } from "../utils";

/*
 * PageHeader — standard dashboard page top bar: page title on the left,
 * action cluster on the right. RSC-safe. Pairs with <PageTitle>.
 */
export type PageHeaderProps = ComponentProps<"div">;

export function PageHeader({ className, ref, ...props }: PageHeaderProps) {
  return (
    <div
      ref={ref}
      className={cn("mb-6 flex items-center justify-between", className)}
      {...props}
    />
  );
}

export default PageHeader;
