"use client";

import { type ReactNode } from "react";
import { Button, type ButtonProps } from "./Button";
import { Tooltip, TooltipContent, TooltipTrigger } from "./Tooltip";

/*
 * IconAction — the standard icon-only row action.
 *
 * One `label` drives both the tooltip body and the button's aria-label, so a
 * glyph-only control can never ship without an accessible name: `label` is
 * required by the type, and the compiler rejects its omission. Size is fixed
 * at `icon-sm` rather than exposed, so every row action in every table is the
 * same size — that uniformity is the point of the primitive.
 *
 * Composition only: this arranges Button inside Tooltip. It re-implements
 * neither. A <TooltipProvider> ancestor is required (mounted at the dashboard
 * layout root).
 *
 *   <IconAction label="Cordon" onClick={cordon}><BanIcon /></IconAction>
 *   <IconAction label="Revoke" variant="destructive"><Trash2Icon /></IconAction>
 */
export interface IconActionProps
  extends Omit<ButtonProps, "size" | "aria-label" | "children"> {
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
    <Tooltip>
      <TooltipTrigger asChild>
        <Button size="icon-sm" variant={variant} aria-label={label} {...rest}>
          {children}
        </Button>
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  );
}

export default IconAction;
