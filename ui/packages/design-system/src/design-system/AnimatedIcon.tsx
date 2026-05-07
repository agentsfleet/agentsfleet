import { type ReactNode } from "react";
import { cn } from "../utils";

type Animation = "wave" | "wiggle" | "drop";
type Trigger = "self-hover" | "parent-hover" | "always";

type AnimatedIconProps = {
  children: ReactNode;
  animation?: Animation;
  trigger?: Trigger;
  label?: string;
  className?: string;
};

/*
 * AnimatedIcon — small animated glyph wrapper.
 *
 * trigger="parent-hover" composes via Tailwind's `group` utility —
 * give an ancestor `className="group"` and its hover/focus will drive
 * the animation. prefers-reduced-motion: reduce in tokens.css neutralizes
 * the animation via [data-animated-glyph].
 */

const animateClass: Record<Animation, string> = {
  wave: "animate-wave",
  wiggle: "animate-wiggle",
  drop: "animate-drop",
};

function triggerClass(trigger: Trigger, animation: Animation): string {
  const anim = animateClass[animation];
  if (trigger === "always") return anim;
  if (trigger === "self-hover") return `hover:${anim} focus-visible:${anim}`;
  return `group-hover:${anim} group-focus-visible:${anim}`;
}

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
      className={cn("inline-flex items-center leading-none", className)}
      aria-hidden={decorative ? "true" : undefined}
      role={decorative ? undefined : "img"}
      aria-label={label}
    >
      <span
        data-animated-glyph=""
        className={cn(
          "inline-block origin-[70%_70%] [will-change:transform]",
          triggerClass(trigger, animation),
        )}
      >
        {children}
      </span>
    </span>
  );
}
