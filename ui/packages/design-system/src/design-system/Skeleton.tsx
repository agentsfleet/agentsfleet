import { type ComponentProps } from "react";
import { cn } from "../utils";

/*
 * Skeleton — content placeholder with pulsing shimmer. RSC-safe, React
 * 19 ref-as-prop. M26.6 may swap `animate-pulse` for a custom
 * `animate-shimmer` keyframe once motion tokens land.
 */
export type SkeletonProps = ComponentProps<"div">;

export function Skeleton({ className, ref, ...props }: SkeletonProps) {
  return (
    <div
      ref={ref}
      className={cn("animate-pulse rounded-md bg-muted", className)}
      {...props}
    />
  );
}

export default Skeleton;
