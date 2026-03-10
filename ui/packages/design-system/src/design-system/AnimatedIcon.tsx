import { type ReactNode } from "react";
import { cn } from "../utils";

type AnimatedIconProps = {
  children: ReactNode;
  animation?: "wave" | "wiggle";
  trigger?: "self-hover" | "parent-hover" | "always";
  label?: string;
  className?: string;
};

export default function AnimatedIcon({
  children,
  animation = "wave",
  trigger = "self-hover",
  label,
  className,
}: AnimatedIconProps) {
  const decorative = !label;

  return (
    <span
      className={cn("z-animated-icon", className)}
      data-animation={animation}
      data-trigger={trigger}
      aria-hidden={decorative ? "true" : undefined}
      role={decorative ? undefined : "img"}
      aria-label={label}
    >
      <span className="z-animated-icon__glyph">{children}</span>
    </span>
  );
}
