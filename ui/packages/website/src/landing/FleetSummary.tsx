import { DescriptionDetails, DescriptionList, DescriptionTerm } from "@agentsfleet/design-system";
import { ClockIcon } from "@phosphor-icons/react/dist/ssr/Clock";
import { KeyIcon } from "@phosphor-icons/react/dist/ssr/Key";

export function FleetSummary({ approved }: { approved: boolean }) {
  return (
    <aside aria-label="Fleet summary" className="rounded-b-lg border-t border-border px-xl py-lg sm:px-2xl">
      <DescriptionList layout="stacked" className="grid space-y-0 grid-cols-2 gap-xl text-label">
        <div className="grid gap-sm">
          <DescriptionTerm className="flex items-center gap-md font-semibold text-text">
            <ClockIcon aria-hidden="true" className="size-4 shrink-0 text-text-muted" />
            Wakes on
          </DescriptionTerm>
          <DescriptionDetails className="text-text-muted">
            {approved ? "Pull request merges" : "No triggers yet"}
          </DescriptionDetails>
        </div>
        <div className="grid gap-sm">
          <DescriptionTerm className="flex items-center gap-md font-semibold text-text">
            <KeyIcon aria-hidden="true" className="size-4 shrink-0 text-text-muted" />
            Can access
          </DescriptionTerm>
          <DescriptionDetails className="text-text-muted">
            {approved ? "GitHub · acme/web" : "No access yet"}
          </DescriptionDetails>
        </div>
      </DescriptionList>
    </aside>
  );
}
