import { cva, type VariantProps } from "class-variance-authority";
import { type ComponentProps } from "react";
import { cn } from "../utils";

/*
 * Toast — transient inline status message announcing the result of a
 * user action. Sister primitive to Alert: Alert is the persistent
 * banner (border + tinted background + padding), Toast is the
 * transient confirmation (color-coded text, no chrome) that
 * auto-dismisses. Caller owns the timing via `visible` + a timer hook
 * (typically `useResettableTimeout`) — this component is the visual +
 * a11y primitive only.
 *
 * Role + aria-live are derived from severity: info/success use polite,
 * warning/destructive use assertive (screen readers interrupt).
 */
export const toastVariants = cva(
  ["font-mono text-mono"],
  {
    variants: {
      severity: {
        info: "text-text-muted",
        success: "text-success",
        warning: "text-warning",
        destructive: "text-destructive",
      },
    },
    defaultVariants: { severity: "info" },
  },
);

export type ToastSeverity = NonNullable<
  VariantProps<typeof toastVariants>["severity"]
>;

export type ToastProps = Omit<ComponentProps<"output">, "children"> &
  VariantProps<typeof toastVariants> & {
    /** True renders the children; false renders the element with no text (preserves layout slot). */
    visible: boolean;
    children: React.ReactNode;
  };

function ariaLiveFor(severity: ToastSeverity): "polite" | "assertive" {
  return severity === "warning" || severity === "destructive"
    ? "assertive"
    : "polite";
}

export function Toast({
  visible,
  severity,
  className,
  children,
  ref,
  ...props
}: ToastProps) {
  const resolved: ToastSeverity = severity ?? "info";
  return (
    <output
      ref={ref}
      aria-live={ariaLiveFor(resolved)}
      aria-atomic="true"
      className={cn(toastVariants({ severity: resolved }), className)}
      {...props}
    >
      {visible ? children : null}
    </output>
  );
}

export default Toast;
