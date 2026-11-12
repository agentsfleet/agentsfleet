import type { ReactNode } from "react";
import { Badge, Section, cn } from "@agentsfleet/design-system";

export function WorkSection({title, children, count, attention = false, id}: {id?: string; title: string; children: ReactNode; count?: number; attention?: boolean}) {
  return <Section asChild className="block gap-0"><section id={id} aria-label={title} className="min-w-0 space-y-sm border-t border-border pt-md first:border-0 first:pt-0">
    <h2 className={cn("flex items-center justify-between gap-md text-eyebrow leading-eyebrow font-medium", attention ? "text-warning" : "text-muted-foreground")}>{title}{count === undefined ? null : <Badge className="h-5.5 min-w-5.5 justify-center border-0 bg-muted p-0 text-eyebrow leading-eyebrow text-current">{count}</Badge>}</h2>{children}
  </section></Section>;
}

