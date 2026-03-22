"use client";

import type { AnchorHTMLAttributes } from "react";
import { trackAppEvent } from "@/lib/analytics/posthog";

type Props = AnchorHTMLAttributes<HTMLAnchorElement> & {
  event: string;
  properties?: Record<string, string | number | boolean | undefined>;
};

export default function TrackedAnchor({ event, properties = {}, onClick, ...props }: Props) {
  return (
    <a
      {...props}
      onClick={(evt) => {
        trackAppEvent(event, properties);
        onClick?.(evt);
      }}
    />
  );
}
