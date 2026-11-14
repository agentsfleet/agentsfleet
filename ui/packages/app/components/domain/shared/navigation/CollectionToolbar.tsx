"use client";

import { Button, Input } from "@agentsfleet/design-system";
import { MagnifyingGlassIcon } from "@phosphor-icons/react/ssr";

type Props = {
  label: string;
  value: string;
  onChange: (value: string) => void;
  summary: string;
};

export function CollectionToolbar({ label, value, onChange, summary }: Props) {
  return <div className="flex flex-wrap items-center justify-between gap-lg">
    <div className="relative flex min-h-11.5 w-full items-center rounded-control border border-border bg-card focus-within:outline-2 focus-within:outline-offset-2 focus-within:outline-ring sm:max-w-search">
      <MagnifyingGlassIcon size={16} className="pointer-events-none absolute left-lg top-1/2 -translate-y-1/2 text-text-dim" aria-hidden="true" />
      <Input type="search" aria-label={label} placeholder={label} value={value}
        onChange={(event) => onChange(event.target.value)} className="min-h-11 border-0 bg-transparent pl-9 focus-visible:ring-0 focus-visible:ring-offset-0" />
    </div>
    <div className="flex items-center gap-md">
      <output className="text-eyebrow text-text-dim">{summary}</output>
      {value ? <Button size="sm" variant="ghost" onClick={() => onChange("")}>Clear search</Button> : null}
    </div>
  </div>;
}
