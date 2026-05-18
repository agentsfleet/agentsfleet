import { cva, type VariantProps } from "class-variance-authority";
import { type ComponentProps, useEffect, useRef, useState } from "react";
import { cn } from "../utils";

// Fade window — keeps children mounted this long after `visible` flips
// false so the opacity transition is perceptible. Must match
// `--motion-duration-fade` in tokens.css (240ms). Exported so tests
// that advance fake timers past the dismiss window stay coupled.
export const TOAST_FADE_MS = 240;

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
 *
 * Layout note: the <output> element renders unconditionally so the
 * a11y live region stays stable across visible/hidden transitions
 * (screen readers attach to a node that exists at mount). After
 * `visible` flips false the children stay mounted for one
 * `TOAST_FADE_MS` window so the opacity transition is perceptible,
 * then unmount — `aria-hidden` flips immediately to suppress a stale
 * screen-reader re-read. `motion-safe:` gates the transition so
 * `prefers-reduced-motion: reduce` users get an instant change. In a
 * fixed-height parent the collapse-to-empty can cause layout shift;
 * wrap in a min-height container if stable layout matters. Hero's
 * `flex flex-wrap` row absorbs the toggle gracefully without a wrapper.
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
  const [rendered, setRendered] = useState(visible);
  const fadeTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    if (visible) {
      setRendered(true);
      if (fadeTimer.current) {
        clearTimeout(fadeTimer.current);
        fadeTimer.current = null;
      }
      return;
    }
    fadeTimer.current = setTimeout(() => setRendered(false), TOAST_FADE_MS);
    return () => {
      if (fadeTimer.current) {
        clearTimeout(fadeTimer.current);
        fadeTimer.current = null;
      }
    };
  }, [visible]);

  return (
    <output
      ref={ref}
      aria-live={ariaLiveFor(resolved)}
      aria-atomic="true"
      aria-hidden={!visible}
      className={cn(
        toastVariants({ severity: resolved }),
        "motion-safe:transition-opacity motion-safe:duration-fade motion-safe:ease-fade",
        visible ? "opacity-100" : "opacity-0",
        className,
      )}
      {...props}
    >
      {rendered ? children : null}
    </output>
  );
}

export default Toast;
