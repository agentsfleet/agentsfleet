"use client";

import { type ReactNode } from "react";
import { TooltipButton, type TooltipButtonProps } from "./TooltipButton";

/*
 * IconAction — the standard icon-only row action.
 *
 * One `label` drives both the tooltip body and the button's aria-label, so a
 * glyph-only control can never ship without an accessible name: `label` is
 * required by the type, and the compiler rejects its omission. Size is fixed
 * at `icon-sm` rather than exposed, so every row action in every table is the
 * same size — that uniformity is the point of the primitive.
 *
 * Composition only: this arranges TooltipButton with the fixed icon size.
 *
 *   <IconAction label="Cordon" onClick={cordon}><PauseIcon /></IconAction>
 *   <IconAction label="Revoke" variant="destructive"><BanIcon /></IconAction>
 */
export interface IconActionProps
  extends Omit<TooltipButtonProps, "size" | "aria-label" | "children" | "tooltip"> {
  /** Accessible name AND tooltip body. Required — an icon alone is not a name. */
  label: string;
  /** The icon glyph. */
  children: ReactNode;
}

export function IconAction({
  label,
  children,
  variant = "outline",
  ...rest
}: IconActionProps) {
  return (
    <TooltipButton
      size="icon-sm"
      variant={variant}
      aria-label={label}
      tooltip={label}
      {...rest}
    >
      {children}
    </TooltipButton>
  );
}

export default IconAction;
