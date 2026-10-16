import type { ReactNode } from "react";
import { Alert, AlertDescription, AlertTitle } from "./Alert";

export type UnavailableStateProps = {
  title: ReactNode;
  description: ReactNode;
  actions?: ReactNode;
};

/** A failed read retains its recovery action and never implies an empty result. */
export function UnavailableState({ title, description, actions }: UnavailableStateProps) {
  return <Alert variant="warning" data-slot="unavailable-state">
    <div className="min-w-0 space-y-lg">
      <div><AlertTitle>{title}</AlertTitle><AlertDescription>{description}</AlertDescription></div>
      {actions ? <div className="flex flex-wrap items-center gap-sm">{actions}</div> : null}
    </div>
  </Alert>;
}
