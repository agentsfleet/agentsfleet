import { Slot } from "@radix-ui/react-slot";
import { cva, type VariantProps } from "class-variance-authority";
import { type ComponentProps, type ReactNode } from "react";
import { cn } from "../utils";

/*
 * Card — unified marketing/dashboard surface. Default <article>; pass
 * asChild for <div>/<section>. Sub-parts mirror shadcn/ui so dashboard
 * layouts port over cleanly.
 */

const cardVariants = cva(
  [
    "relative rounded-lg border border-border bg-card p-6",
    "transition-[border-color,box-shadow] duration-200 ease-out",
    "hover:border-border-active hover:shadow-card",
  ].join(" "),
  {
    variants: {
      featured: {
        true: "border-primary shadow-card",
        false: "",
      },
    },
    defaultVariants: { featured: false },
  },
);

export type CardProps = ComponentProps<"article"> &
  VariantProps<typeof cardVariants> & {
    /** Badge label rendered above a featured card. Defaults to "Popular". */
    badgeLabel?: ReactNode;
    asChild?: boolean;
  };

export function Card({
  featured,
  badgeLabel,
  asChild = false,
  className,
  children,
  ref,
  ...props
}: CardProps) {
  const classes = cn(cardVariants({ featured }), className);

  if (asChild) {
    return (
      <Slot ref={ref} className={classes} {...props}>
        {children}
      </Slot>
    );
  }

  return (
    <article ref={ref} className={classes} {...props}>
      {featured ? (
        <span
          aria-hidden="true"
          className={cn(
            "absolute -top-2.5 left-6 rounded-full px-2.5 py-0.5",
            "bg-[linear-gradient(120deg,var(--primary),var(--primary-bright))]",
            "text-[0.7rem] font-bold uppercase tracking-wider text-primary-foreground",
          )}
        >
          {badgeLabel ?? "Popular"}
        </span>
      ) : null}
      {children}
    </article>
  );
}

export function CardHeader({ className, ref, ...props }: ComponentProps<"div">) {
  return <div ref={ref} className={cn("flex flex-col gap-1.5 pb-3", className)} {...props} />;
}

export function CardTitle({ className, ref, ...props }: ComponentProps<"div">) {
  return (
    <div ref={ref} className={cn("font-semibold leading-none tracking-tight", className)} {...props} />
  );
}

export function CardDescription({ className, ref, ...props }: ComponentProps<"div">) {
  return <div ref={ref} className={cn("text-sm text-muted-foreground", className)} {...props} />;
}

export function CardContent({ className, ref, ...props }: ComponentProps<"div">) {
  return <div ref={ref} className={cn("pt-0", className)} {...props} />;
}

export function CardFooter({ className, ref, ...props }: ComponentProps<"div">) {
  return <div ref={ref} className={cn("flex items-center pt-4", className)} {...props} />;
}

export { cardVariants };
export default Card;
