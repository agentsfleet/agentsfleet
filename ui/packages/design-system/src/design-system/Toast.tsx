import { cva, type VariantProps } from "class-variance-authority";
import { type ComponentProps, useEffect, useRef, useState } from "react";
import { cn } from "../utils";

// Default fade window — mirrors the `--motion-duration-fade` design
// token (tokens.css). Used when the caller does not pass `fadeMs`; per-
// instance overrides flow through the prop so both the JS setTimeout
// and the inline CSS transitionDuration share one source.
const DEFAULT_FADE_MS = 240;

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
 * `visible` flips false the children stay mounted for one `fadeMs`
 * window so the opacity transition is perceptible, then unmount —
 * `aria-hidden` flips immediately to suppress a stale screen-reader
 * re-read. The same `fadeMs` value drives the inline CSS
 * transitionDuration so the visual fade and the JS unmount stay in
 * lockstep regardless of theme overrides. `motion-safe:` gates the
 * transition so `prefers-reduced-motion: reduce` users get an instant
 * change. In a fixed-height parent the collapse-to-empty can cause
 * layout shift; wrap in a min-height container if stable layout matters.
 * Hero's `flex flex-wrap` row absorbs the toggle gracefully without a
 * wrapper.
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
    /** True renders the children; false starts the fade-out + unmount cycle. */
    visible: boolean;
    /**
     * Fade-out duration in milliseconds. Drives BOTH the inline CSS
     * `transition-duration` AND the JS setTimeout that unmounts the
     * children after the fade completes — single source so the visual
     * and the DOM cannot drift. Defaults to 240 (mirrors the
     * `--motion-duration-fade` token).
     */
    fadeMs?: number;
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
  fadeMs = DEFAULT_FADE_MS,
  className,
  style,
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
    fadeTimer.current = setTimeout(() => setRendered(false), fadeMs);
    return () => {
      if (fadeTimer.current) {
        clearTimeout(fadeTimer.current);
        fadeTimer.current = null;
      }
    };
  }, [visible, fadeMs]);

  return (
    <output
      ref={ref}
      aria-live={ariaLiveFor(resolved)}
      aria-atomic="true"
      aria-hidden={!visible}
      className={cn(
        toastVariants({ severity: resolved }),
        "motion-safe:transition-opacity motion-safe:ease-fade",
        visible ? "opacity-100" : "opacity-0",
        className,
      )}
      style={{ transitionDuration: `${fadeMs}ms`, ...style }}
      {...props}
    >
      {rendered ? children : null}
    </output>
  );
}

export default Toast;
