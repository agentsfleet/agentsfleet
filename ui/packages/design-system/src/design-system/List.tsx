import { Slot } from "@radix-ui/react-slot";
import { cva, type VariantProps } from "class-variance-authority";
import { type ComponentProps, type ReactNode } from "react";
import { cn } from "../utils";

/*
 * List + ListItem — semantic listing primitive. `unordered` (default)
 * renders <ul> with disc bullets; `ordered` renders <ol> with decimal;
 * `plain` renders <ul> with no marker (defeats Tailwind preflight).
 * `divided` adds a bottom border on every item except the last.
 *
 * `asChild` swaps the host element (e.g. <menu>) while preserving the
 * variant classes — for the rare case the caller needs a non-ul/ol tag.
 *
 * Default `role="list"` preserves list semantics in Safari VoiceOver,
 * which strips the implicit role from any <ul>/<ol> with list-style:none
 * applied via CSS. Pass an explicit `role` to override.
 */

export const listVariants = cva("space-y-2 text-sm", {
  variants: {
    variant: {
      unordered: "list-disc pl-5 marker:text-muted-foreground",
      ordered: "list-decimal pl-5 marker:text-muted-foreground",
      plain: "list-none pl-0",
    },
    divided: {
      true: "[&>li:not(:last-child)]:border-b [&>li:not(:last-child)]:border-border [&>li:not(:last-child)]:pb-2",
      false: "",
    },
  },
  defaultVariants: { variant: "unordered", divided: false },
});

export type ListVariant = NonNullable<VariantProps<typeof listVariants>["variant"]>;

export interface ListProps
  extends Omit<ComponentProps<"ul">, "children">,
    VariantProps<typeof listVariants> {
  children: ReactNode;
  asChild?: boolean;
}

export function List({
  variant = "unordered",
  divided = false,
  asChild = false,
  role = "list",
  className,
  children,
  ref,
  ...props
}: ListProps) {
  const classes = cn(listVariants({ variant, divided }), className);

  if (asChild) {
    return (
      <Slot ref={ref} role={role} className={classes} {...props}>
        {children}
      </Slot>
    );
  }

  if (variant === "ordered") {
    return (
      <ol
        ref={ref as ComponentProps<"ol">["ref"]}
        role={role}
        className={classes}
        {...(props as ComponentProps<"ol">)}
      >
        {children}
      </ol>
    );
  }

  return (
    <ul ref={ref} role={role} className={classes} {...props}>
      {children}
    </ul>
  );
}

export function ListItem({ className, ref, ...props }: ComponentProps<"li">) {
  return <li ref={ref} className={cn(className)} {...props} />;
}

export default List;
