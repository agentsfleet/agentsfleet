"use client";

import * as LabelPrimitive from "@radix-ui/react-label";
import { type ComponentProps } from "react";
import { cn } from "../utils";

/*
 * Label — Radix Label primitive with semantic utilities. Client boundary
 * because Radix Label uses DOM events to forward focus to its associated
 * input. React 19 ref-as-prop.
 */

export type LabelProps = ComponentProps<typeof LabelPrimitive.Root> & { appearance?: "default" | "filter" };

export function Label({ appearance = "default", className, ref, ...props }: LabelProps) {
  return (
    <LabelPrimitive.Root
      ref={ref}
      className={cn(
        "block font-sans leading-label",
        appearance === "filter" ? "text-eyebrow font-normal text-muted-foreground" : "text-label font-medium text-foreground",
        "peer-disabled:cursor-not-allowed peer-disabled:opacity-70",
        className,
      )}
      {...props}
    />
  );
}

export default Label;
