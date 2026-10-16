"use client";

import { Button, Input } from "@agentsfleet/design-system";
import { SearchIcon } from "lucide-react";

type Props = {
  label: string;
  value: string;
  onChange: (value: string) => void;
  summary: string;
};

export function CollectionToolbar({ label, value, onChange, summary }: Props) {
  return <div className="flex flex-wrap items-center justify-between gap-lg">
    <div className="relative w-full sm:max-w-xs">
      <SearchIcon size={16} className="pointer-events-none absolute left-lg top-1/2 -translate-y-1/2 text-muted-foreground" aria-hidden="true" />
      <Input type="search" aria-label={label} placeholder={label} value={value}
        onChange={(event) => onChange(event.target.value)} className="pl-9" />
    </div>
    <div className="flex items-center gap-md">
      <output className="text-label tabular-nums text-muted-foreground">{summary}</output>
      {value ? <Button size="sm" variant="ghost" onClick={() => onChange("")}>Clear search</Button> : null}
    </div>
  </div>;
}
