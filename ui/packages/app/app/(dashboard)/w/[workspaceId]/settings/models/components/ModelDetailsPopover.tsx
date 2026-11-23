"use client";

import type { LibraryModel } from "@/lib/api/model-library-types";
import { useId, useState } from "react";
import { InfoIcon, XIcon } from "@phosphor-icons/react/ssr";
import { Button, IconAction, Popover, PopoverContent, PopoverTrigger } from "@agentsfleet/design-system";
import { CatalogueModelDetails } from "@/components/domain/models/CatalogueModelDetails";

export function ModelDetailsPopover({model}: {model: LibraryModel}) {
  const title = useId();
  const [open, setOpen] = useState(false);

  return <Popover open={open} onOpenChange={setOpen}>
    <PopoverTrigger asChild>
      <IconAction type="button" variant="ghost" label="Model details" className="absolute -bottom-md right-0">
        <InfoIcon aria-hidden="true" />
      </IconAction>
    </PopoverTrigger>
    <PopoverContent align="end" aria-labelledby={title}
      className="w-105 max-h-[min(28rem,var(--radix-popover-content-available-height))] space-y-lg overflow-y-auto overscroll-contain p-xl">
      <div className="flex items-center justify-between gap-lg">
        <h3 id={title} className="text-label font-title">Model details</h3>
        <Button type="button" variant="ghost" size="icon-sm" aria-label="Close model details" onClick={() => setOpen(false)}>
          <XIcon aria-hidden="true" />
        </Button>
      </div>
      <CatalogueModelDetails model={model} />
    </PopoverContent>
  </Popover>;
}
