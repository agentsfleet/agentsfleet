"use client";

import * as SwitchPrimitive from "@radix-ui/react-switch";
import { type ComponentProps } from "react";
import { cn } from "../utils";

/*
 * Switch — Radix Switch composition: an on/off setting that applies at once,
 * announced as role="switch" with aria-checked. Client boundary because Radix
 * owns the controlled/uncontrolled state and the button's keyboard handling.
 * On reads as the pulse, the colour of a selected thing; off sits on the
 * strong border so the track stays visible on a card in both themes.
 *
 * Compose: <Switch id="x" checked={on} onCheckedChange={setOn} />
 *          <Label htmlFor="x">Name</Label>
 *
 * The track is 36x20; on a coarse pointer an overlay widens what a finger can
 * hit to 48x48 without changing what is drawn (DESIGN_SYSTEM.md's 44px floor,
 * CopyButton's overlay). aria-disabled looks like disabled, for a caller that
 * must keep the switch focusable while a change is saving.
 */

export type SwitchProps = ComponentProps<typeof SwitchPrimitive.Root>;

const TRACK_CLASS = "inline-flex h-5 w-9 shrink-0 items-center rounded-full border-2 border-transparent bg-border-strong data-[state=checked]:bg-primary transition-colors duration-snap ease-snap";
const THUMB_CLASS = "pointer-events-none block h-4 w-4 rounded-full bg-background data-[state=checked]:translate-x-4 data-[state=checked]:bg-primary-foreground transition-transform duration-snap ease-snap";

/** The switch appearance for a menu item that already owns keyboard input. */
export function SwitchIndicator({checked}: {checked: boolean}) {
  const state = checked ? "checked" : "unchecked";
  return <span aria-hidden="true" data-state={state} className={TRACK_CLASS}>
    <span data-state={state} className={THUMB_CLASS} />
  </span>;
}

export function Switch({ className, ref, ...props }: SwitchProps) {
  return (
    <SwitchPrimitive.Root
      ref={ref}
      className={cn(
        TRACK_CLASS, "cursor-pointer",
        "relative after:absolute after:inset-0 pointer-coarse:after:-inset-y-xl pointer-coarse:after:-inset-x-md",
        "ring-offset-background focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2",
        "disabled:cursor-not-allowed disabled:opacity-50 aria-disabled:cursor-not-allowed aria-disabled:opacity-50",
        className,
      )}
      {...props}
    >
      <SwitchPrimitive.Thumb className={THUMB_CLASS} />
    </SwitchPrimitive.Root>
  );
}
