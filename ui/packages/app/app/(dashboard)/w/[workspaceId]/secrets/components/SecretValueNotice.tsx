import { CircleHelpIcon } from "lucide-react";

export function SecretValueNotice() {
  return <>Values are encrypted on save — you can replace them later,{" "}
    <span className="font-semibold text-info">but never view them again.</span>{" "}
    <a href="https://docs.agentsfleet.net/fleets/secrets" target="_blank" rel="noopener noreferrer"
      className="inline-flex items-center gap-1 text-pulse underline-offset-2 hover:underline focus-visible:underline">
      <CircleHelpIcon size={13} aria-hidden="true" />
      Learn more<span className="sr-only"> (opens in a new tab)</span>
    </a>
  </>;
}
