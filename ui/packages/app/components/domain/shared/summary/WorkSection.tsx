import { use, type ReactNode } from "react";
import { Badge, Section, cn } from "@agentsfleet/design-system";
import { CurrentWorkSummaryContext } from "./FleetWorkSidebar";

export function WorkSection({title, children, count, attention = false, id}: {id?: string; title: string; children: ReactNode; count?: number; attention?: boolean}) {
  const currentWork = use(CurrentWorkSummaryContext);
  return <Section id={id} aria-label={title} variant="divided" className={cn(
    currentWork ? "pt-md not-last:pb-md summary:pt-lg summary:not-last:pb-lg" : "space-y-md pt-md first:border-0 first:pt-0")}>
    <h2 className={cn("flex items-center justify-between gap-md text-eyebrow leading-eyebrow font-medium", currentWork && "mb-md", attention ? "text-warning" : "text-text-dim")}>
      {title}
      {count === undefined ? null : <Badge className="h-5.5 min-w-5.5 justify-center border-0 bg-muted p-0 text-eyebrow leading-eyebrow text-current">{count}</Badge>}
    </h2>
    {children}
  </Section>;
}

