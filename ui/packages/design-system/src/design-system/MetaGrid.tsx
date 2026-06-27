import { type ComponentProps, type ReactNode } from "react";

import { cn } from "../utils";

export type MetaGridItem = {
  label: string;
  value: ReactNode;
};

export type MetaGridProps = ComponentProps<"dl"> & {
  items: readonly MetaGridItem[];
  columns?: 2 | 3;
  bordered?: boolean;
};

const columnsClass = {
  2: "sm:grid-cols-2",
  3: "sm:grid-cols-3",
} as const;

export function MetaGrid({
  items,
  columns = 3,
  bordered = false,
  className,
  ref,
  ...props
}: MetaGridProps) {
  return (
    <dl
      ref={ref}
      className={cn(
        "grid gap-3 text-body-sm",
        columnsClass[columns],
        bordered ? "border-t border-border pt-lg" : null,
        className,
      )}
      {...props}
    >
      {items.map((item) => (
        <div key={item.label}>
          <dt className="font-mono text-label uppercase tracking-label text-muted-foreground">
            {item.label}
          </dt>
          <dd className="mt-1 text-foreground">{item.value}</dd>
        </div>
      ))}
    </dl>
  );
}
