import type { ReactNode } from "react";
import { DisplayXL } from "@agentsfleet/design-system";

export const LEGAL_HEADING =
  "mt-lg border-t border-border pt-lg text-heading leading-heading font-bold text-text";

export function LegalPage({
  title,
  updated,
  testId,
  children,
}: {
  title: string;
  updated: string;
  testId: string;
  children: ReactNode;
}) {
  return (
    <article
      data-testid={testId}
      className="mx-auto grid w-full max-w-prose gap-lg px-xl py-4xl leading-prose text-text-muted sm:px-3xl"
    >
      <header className="mb-lg grid gap-md">
        <DisplayXL className="text-fluid-display-lg font-bold">
          {title}
        </DisplayXL>
        <p className="font-mono text-label">Last updated: {updated}</p>
      </header>
      {children}
    </article>
  );
}
