import { useEffect, useRef } from "react";
import { SwitchIndicator } from "@agentsfleet/design-system";
import { GitBranchIcon } from "@phosphor-icons/react/dist/ssr/GitBranch";
import { KeyIcon } from "@phosphor-icons/react/dist/ssr/Key";
import { ListBulletsIcon } from "@phosphor-icons/react/dist/ssr/ListBullets";

const settings = [
  ["Model", "Default model"],
  ["Spending limit", "$3.00 / day"],
  ["Next wake-up", "09:37 AM IST"],
] as const;

const networkSettings = ["Allow all", "Enable package manager domains"];

export function FleetSummary({ onInspect }: { onInspect: () => void }) {
  const disclosure = useRef<HTMLDetailsElement>(null);
  useEffect(() => {
    const details = disclosure.current!;
    const wide = window.matchMedia("(min-width: 48rem)");
    const update = () => {
      details.open = wide.matches;
    };
    update();
    wide.addEventListener("change", update);
    return () => wide.removeEventListener("change", update);
  }, []);
  return (
    <aside
      className="min-w-0 rounded-md border border-zinc-700 bg-zinc-800/60 px-lg text-label leading-label text-zinc-100"
      aria-label="Scout fleet setup summary"
    >
      <details ref={disclosure}>
        <summary
          onClick={onInspect}
          className="flex min-h-11 cursor-pointer list-none items-center justify-between gap-md font-semibold focus-visible:outline-2 focus-visible:outline-emerald-200 [&::-webkit-details-marker]:hidden"
        >
          Summary
          <ListBulletsIcon
            aria-hidden="true"
            className="size-4 shrink-0 text-zinc-300"
          />
        </summary>
        <div className="divide-y divide-zinc-700">
          <dl className="grid gap-sm pb-lg">
            {settings.map(([label, value]) => (
              <div
                key={label}
                className="flex flex-wrap items-baseline justify-between gap-x-md"
              >
                <dt className="text-zinc-300">{label}</dt>
                <dd className="font-medium">{value}</dd>
              </div>
            ))}
          </dl>
          <div className="grid gap-sm py-lg">
            <h3 className="font-semibold">Repositories</h3>
            <p className="flex min-w-0 items-start gap-md text-zinc-200">
              <GitBranchIcon
                aria-hidden="true"
                className="mt-px size-4 shrink-0"
              />
              agentsfleet/agentsfleet
            </p>
            <p className="text-zinc-300">
              Tools{" "}
              <span className="text-zinc-100">HTTP request · +3 more</span>
            </p>
          </div>
          <div className="grid gap-md py-lg">
            <h3 className="font-semibold">Public internet access</h3>
            {networkSettings.map((label) => (
              <div
                key={label}
                className="flex items-center justify-between gap-md text-zinc-300"
              >
                <span>
                  {label}
                  <span className="sr-only"> off</span>
                </span>
                <SwitchIndicator checked={false} />
              </div>
            ))}
          </div>
          <div className="grid gap-sm py-lg">
            <h3 className="font-semibold">Secrets</h3>
            <p className="flex items-center gap-md text-zinc-200">
              <KeyIcon aria-hidden="true" className="size-4 shrink-0" />
              grafana
            </p>
          </div>
        </div>
      </details>
    </aside>
  );
}
