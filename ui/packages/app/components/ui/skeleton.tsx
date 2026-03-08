import { cn } from "@/lib/utils";

function Skeleton({ className, ...props }: React.HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      className={cn(
        "animate-pulse rounded-md bg-[var(--z-surface-1)]",
        className,
      )}
      {...props}
    />
  );
}

export { Skeleton };
