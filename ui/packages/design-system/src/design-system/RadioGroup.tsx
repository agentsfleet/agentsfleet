"use client";

import * as RadioGroupPrimitive from "@radix-ui/react-radio-group";
import { type ComponentProps } from "react";
import { cn } from "../utils";

/*
 * RadioGroup — Radix RadioGroup composition. Client boundary because
 * Radix uses DOM APIs for arrow-key navigation, roving tab-index, and
 * the controlled/uncontrolled value forwarding. React 19 ref-as-prop.
 *
 * Compose: <RadioGroup value={v} onValueChange={setV}>
 *   <RadioGroupItem value="a" id="a" /> <Label htmlFor="a">A</Label>
 *   <RadioGroupItem value="b" id="b" /> <Label htmlFor="b">B</Label>
 * </RadioGroup>
 */

export type RadioGroupProps = ComponentProps<typeof RadioGroupPrimitive.Root>;

export function RadioGroup({ className, ref, ...props }: RadioGroupProps) {
  return (
    <RadioGroupPrimitive.Root
      ref={ref}
      className={cn("grid gap-2", className)}
      {...props}
    />
  );
}

export type RadioGroupItemProps = ComponentProps<typeof RadioGroupPrimitive.Item>;

export function RadioGroupItem({ className, ref, ...props }: RadioGroupItemProps) {
  return (
    <RadioGroupPrimitive.Item
      ref={ref}
      className={cn(
        "aspect-square h-4 w-4 rounded-full border border-border text-primary",
        "ring-offset-background",
        "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2",
        "disabled:cursor-not-allowed disabled:opacity-50",
        "data-[state=checked]:border-primary data-[state=checked]:bg-primary/10",
        "transition-colors duration-150",
        className,
      )}
      {...props}
    >
      <RadioGroupPrimitive.Indicator className="flex items-center justify-center">
        <span className="h-2 w-2 rounded-full bg-primary" />
      </RadioGroupPrimitive.Indicator>
    </RadioGroupPrimitive.Item>
  );
}
