"use client";

import { useState } from "react";
import {
  PageHeader,
  PageTitle,
  Section,
  SectionLabel,
  TooltipButton,
} from "@agentsfleet/design-system";
import { PlusIcon } from "lucide-react";
import type { PlatformCatalogEntry } from "@/lib/types";
import {
  ADD_FLEET,
  ADD_TOOLTIP,
  FLEET_LIBRARIES_DESCRIPTION,
  FLEET_LIBRARIES_TITLE,
} from "../library-copy";
import AddFleetDialog from "./AddFleetDialog";
import PlatformCatalogTable from "./PlatformCatalogTable";

// The catalog IS the page. Every write revalidates it, so an operator never has to
// guess whether the thing they just did took — which is what the previous
// session-scoped result card could not tell them.
export default function FleetLibrariesView({ entries }: { entries: PlatformCatalogEntry[] }) {
  const [adding, setAdding] = useState(false);
  // Set when the dialog was opened from a row's Fetch action, so the operator
  // never retypes a repository the table is already showing them.
  const [prefillRepo, setPrefillRepo] = useState<string | undefined>(undefined);

  function openAdd() {
    setPrefillRepo(undefined);
    setAdding(true);
  }

  function openFetch(entry: PlatformCatalogEntry) {
    setPrefillRepo(entry.source_repo);
    setAdding(true);
  }

  return (
    <div className="space-y-8">
      <PageHeader description={FLEET_LIBRARIES_DESCRIPTION}>
        <PageTitle>{FLEET_LIBRARIES_TITLE}</PageTitle>
      </PageHeader>

      <Section aria-label="Platform fleet catalog">
        <div className="flex flex-wrap items-baseline justify-between gap-md">
          <SectionLabel>{FLEET_LIBRARIES_TITLE}</SectionLabel>
          <TooltipButton type="button" size="sm" tooltip={ADD_TOOLTIP} onClick={openAdd}>
            <PlusIcon size={14} />
            {ADD_FLEET}
          </TooltipButton>
        </div>

        <PlatformCatalogTable entries={entries} onFetch={openFetch} />

        <AddFleetDialog open={adding} onOpenChange={setAdding} prefillRepo={prefillRepo} />
      </Section>
    </div>
  );
}
