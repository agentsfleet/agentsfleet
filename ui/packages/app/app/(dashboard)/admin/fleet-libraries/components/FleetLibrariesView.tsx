"use client";

import { useState } from "react";
import {
  PageHeader,
  PageLayout,
  PageTitle,
  Section,
  SectionHeader,
  TooltipButton,
} from "@agentsfleet/design-system";
import { PlusIcon } from "lucide-react";
import type { PlatformCatalogEntry } from "@/lib/types";
import {
  ADD_TOOLTIP,
  CREATE_FLEET_LIBRARY,
  FLEET_LIBRARIES_DESCRIPTION,
  FLEET_LIBRARY_TITLE,
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
  // The stored ref rides along so a PATCH-pinned branch/tag is what the refetch
  // actually fetches — not silently the default branch.
  const [prefillRef, setPrefillRef] = useState<string | undefined>(undefined);

  function openAdd() {
    setPrefillRepo(undefined);
    setPrefillRef(undefined);
    setAdding(true);
  }

  function openFetch(entry: PlatformCatalogEntry) {
    setPrefillRepo(entry.source_repo);
    setPrefillRef(entry.source_ref);
    setAdding(true);
  }

  return (
    <PageLayout>
      <PageHeader description={FLEET_LIBRARIES_DESCRIPTION}>
        <PageTitle>{FLEET_LIBRARY_TITLE}</PageTitle>
      </PageHeader>

      <Section aria-label="Platform fleet catalog">
        <SectionHeader
          actions={
            <TooltipButton type="button" size="sm" tooltip={ADD_TOOLTIP} onClick={openAdd}>
              <PlusIcon size={14} />
              {CREATE_FLEET_LIBRARY}
            </TooltipButton>
          }
        >
          {FLEET_LIBRARY_TITLE}
        </SectionHeader>

        <PlatformCatalogTable entries={entries} onFetch={openFetch} />

        <AddFleetDialog open={adding} onOpenChange={setAdding} prefillRepo={prefillRepo} prefillRef={prefillRef} />
      </Section>
    </PageLayout>
  );
}
