import { type ComponentProps } from "react";
import { cn } from "../utils";

const CONTENT_LAYOUT_CLASS = "flex min-w-0 flex-col gap-8";
const FULL_HEIGHT_LAYOUT_CLASS = "min-h-0 flex-1";

export type PageLayoutProps = ComponentProps<"div"> & {
  fullHeight?: boolean;
};

export function PageLayout({
  className,
  fullHeight = false,
  ref,
  ...props
}: PageLayoutProps) {
  return (
    <div
      ref={ref}
      className={cn(
        CONTENT_LAYOUT_CLASS,
        fullHeight && FULL_HEIGHT_LAYOUT_CLASS,
        className,
      )}
      data-page-layout={fullHeight ? "full-height" : undefined}
      {...props}
    />
  );
}

export default PageLayout;
