"use client";

import { useId, type ReactNode } from "react";
import { Section, SectionHeader } from "@agentsfleet/design-system";

export function PolicySection({ title, children }: {
  title: string;
  children: ReactNode;
}) {
  const headingId = useId();
  return <Section aria-labelledby={headingId} className="gap-lg py-xl first:pt-0 last:pb-0">
    <SectionHeader id={headingId} className="text-muted-foreground">
      {title}
    </SectionHeader>
    {children}
  </Section>;
}
