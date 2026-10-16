"use client";

import * as LabelPrimitive from "@radix-ui/react-label";
import { type ComponentProps } from "react";
import { cn } from "../utils";

/*
 * Label — Radix Label primitive with semantic utilities. Client boundary
 * because Radix Label uses DOM events to forward focus to its associated
 * input. React 19 ref-as-prop.
 */

export type LabelProps = ComponentProps<typeof LabelPrimitive.Root>;

export function Label({ className, ref, ...props }: LabelProps) {
  return (
    <LabelPrimitive.Root
      ref={ref}
      className={cn(
        "font-sans text-label font-semibold leading-label text-foreground",
        "peer-disabled:cursor-not-allowed peer-disabled:opacity-70",
        className,
      )}
      {...props}
    />
  );
}

export default Label;
