import type { SVGProps } from "react";
import { cn } from "../utils";

const RAYS = ["M12 2v6", "M19.07 4.93l-4.24 4.24", "M22 12h-6", "M19.07 19.07l-4.24-4.24", "M12 22v-6", "M4.93 19.07l4.24-4.24", "M2 12h6", "M4.93 4.93l4.24 4.24"];

/** Static identity. Motion is opt-in through SignalSpinner. */
export function BrandMark({ className, ...props }: SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 24 24" width="24" height="24" fill="none" aria-hidden="true" focusable="false"
      className={cn("shrink-0 text-pulse", className)} data-brand-mark="signal" {...props}>
      {RAYS.map((d) => <path key={d} d={d} stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />)}
      <circle cx="12" cy="12" r="1.5" fill="currentColor" />
    </svg>
  );
}

/** Decorative waiting mark; its adjacent label supplies the accessible status. */
export function SignalSpinner({ className, ...props }: SVGProps<SVGSVGElement>) {
  return <BrandMark data-signal-spinner="" className={cn("size-4", className)} {...props} />;
}
