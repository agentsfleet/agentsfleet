import { Slot } from "@radix-ui/react-slot";
import { cva, type VariantProps } from "class-variance-authority";
import { type ComponentProps } from "react";
import { cn } from "../utils";

/* Inline X — design-system avoids lucide-react as a transitive dep. */
function DismissIcon() {
  return (
    <svg
      aria-hidden="true"
      width="16"
      height="16"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d="M18 6 6 18" />
      <path d="m6 6 12 12" />
    </svg>
  );
}

/*
 * Alert — page-level status banner. Variants map to semantic theme
 * tokens (info/success/warning/destructive). Default role is chosen
 * by severity (alert for destructive/warning, status otherwise) and
 * can be overridden per call site. Toast-style auto-dismiss is out
 * of scope; opt-in manual dismiss via `onDismiss`.
 */

export const alertVariants = cva(
  [
    "relative flex items-start gap-3 rounded-md border px-4 py-3 text-sm",
    "animate-in fade-in-0 duration-200",
  ].join(" "),
  {
    variants: {
      variant: {
        info: "border-info/40 bg-info/10 text-info",
        success: "border-success/40 bg-success/10 text-success",
        warning: "border-warning/40 bg-warning/10 text-warning",
        destructive:
          "border-destructive/40 bg-destructive/10 text-destructive",
      },
    },
    defaultVariants: { variant: "info" },
  },
);

export type AlertVariant = NonNullable<VariantProps<typeof alertVariants>["variant"]>;

type AlertBaseProps = Omit<ComponentProps<"div">, "role"> &
  VariantProps<typeof alertVariants> & {
    /** Override the default role (alert for destructive/warning, status otherwise). */
    role?: "alert" | "status";
  };

/*
 * `asChild` and `onDismiss` are mutually exclusive: when the alert
 * renders as its child element via Radix Slot there is no surrounding
 * <div> to host a dismiss button, so accepting `onDismiss` would be a
 * silent no-op. Encoded as a discriminated union so the compiler
 * rejects `<Alert asChild onDismiss={...}>` at the call site.
 */
export type AlertProps =
  | (AlertBaseProps & {
      asChild?: false;
      /** Optional dismiss handler. Renders an X button when provided. */
      onDismiss?: () => void;
    })
  | (AlertBaseProps & {
      /** Render the alert as the child element instead of a div. */
      asChild: true;
      /** Not supported with `asChild` — there is no host div for the dismiss button. */
      onDismiss?: never;
    });

function defaultRole(variant: AlertVariant | null | undefined): "alert" | "status" {
  return variant === "destructive" || variant === "warning" ? "alert" : "status";
}

export function Alert({
  variant,
  onDismiss,
  role,
  asChild = false,
  className,
  children,
  ref,
  ...props
}: AlertProps) {
  const resolvedVariant: AlertVariant = variant ?? "info";
  const resolvedRole = role ?? defaultRole(resolvedVariant);
  const classes = cn(alertVariants({ variant: resolvedVariant }), className);

  if (asChild) {
    return (
      <Slot ref={ref} role={resolvedRole} className={classes} {...props}>
        {children}
      </Slot>
    );
  }

  return (
    <div ref={ref} role={resolvedRole} className={classes} {...props}>
      {children}
      {onDismiss ? (
        <button
          type="button"
          aria-label="Dismiss"
          onClick={onDismiss}
          className={cn(
            "ml-auto shrink-0 rounded-sm opacity-70 transition-opacity",
            "hover:opacity-100 focus:outline-none focus:ring-2 focus:ring-current",
          )}
        >
          <DismissIcon />
        </button>
      ) : null}
    </div>
  );
}

export function AlertTitle({ className, ref, ...props }: ComponentProps<"div">) {
  return (
    <div
      ref={ref}
      className={cn("font-semibold leading-tight tracking-tight", className)}
      {...props}
    />
  );
}

export function AlertDescription({ className, ref, ...props }: ComponentProps<"div">) {
  return (
    <div
      ref={ref}
      className={cn("mt-1 text-sm opacity-90", className)}
      {...props}
    />
  );
}

export default Alert;
