"use client";

import { useState } from "react";
import {
  Badge,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DescriptionDetails,
  DescriptionList,
  DescriptionTerm,
  Time,
} from "@agentsfleet/design-system";
import { providerLabel } from "@/lib/api/model-library-types";
import { CatalogueModelDetails } from "@/components/domain/CatalogueModelDetails";
import { CatalogueBrowseControls } from "./CatalogueBrowseControls";
import { useModelCatalogue } from "./ModelCatalogueProvider";
import { libraryRateFor } from "./registry-view";
import { CATALOGUE_STATUS } from "./catalogue-status";
import { modelLabel } from "@/lib/models/display";
import type { TenantModelEntry } from "@/lib/types";

type Props = {
  target: TenantModelEntry | null;
  onOpenChange: (open: boolean) => void;
};

/** Rendered only while `target` is non-null (see the Dialog body below). */
function Details({ target }: { target: TenantModelEntry }) {
  return (
    <>
      <DialogHeader>
        <DialogTitle>{modelLabel(target.model_id, target.display_name)}</DialogTitle>
        {/* Creation time and key presence are header context, not rows: the rows
            say what the entry is, the header says when it landed and whether a
            key backs it. */}
        <div className="flex items-center justify-between gap-2">
          <DialogDescription>
            Added <Time value={new Date(target.created_at)} format="relative" />
          </DialogDescription>
          <Badge variant={target.has_key ? "green" : "default"}>
            {target.has_key ? "In vault" : "Keyless endpoint"}
          </Badge>
        </div>
      </DialogHeader>
      <DescriptionList>
        <div>
          <DescriptionTerm>Provider</DescriptionTerm>
          <DescriptionDetails>{target.provider ? providerLabel(target.provider) : "Unknown"}</DescriptionDetails>
        </div>
        <div>
          <DescriptionTerm>Model</DescriptionTerm>
          <DescriptionDetails mono>{target.model_id}</DescriptionDetails>
        </div>
        {/* `secret_ref` is the vault key reference, not a display name — labelling
            it "Name" made it read as a duplicate of Provider. */}
        <div>
          <DescriptionTerm>Secret ref</DescriptionTerm>
          <DescriptionDetails mono>{target.secret_ref}</DescriptionDetails>
        </div>
        {target.base_url ? (
          <div>
            <DescriptionTerm>Endpoint</DescriptionTerm>
            <DescriptionDetails mono>{target.base_url}</DescriptionDetails>
          </div>
        ) : null}
      </DescriptionList>
      <CatalogueDetails target={target} />
    </>
  );
}

function CatalogueDetails({ target }: { target: TenantModelEntry }) {
  const { models, status } = useModelCatalogue();
  const [advanced, setAdvanced] = useState(false);
  const catalogue = libraryRateFor(models, target.provider, target.model_id);
  return <div className="space-y-sm">
    {catalogue ? <CatalogueModelDetails model={catalogue} /> : <DialogDescription>
      {status === CATALOGUE_STATUS.loading ? "Loading catalogue details…" : "Catalogue evidence and price schedule are unavailable for this model."}
    </DialogDescription>}
    <CatalogueBrowseControls id={`details-${target.id}-advanced`} advanced={advanced} onAdvancedChange={setAdvanced} />
  </div>;
}

/** Read-only detail view for one registry row — never the key material itself. */
export default function ModelDetailsDialog({ target, onOpenChange }: Props) {
  return (
    <Dialog open={target !== null} onOpenChange={onOpenChange}>
      <DialogContent>{target ? <Details target={target} /> : null}</DialogContent>
    </Dialog>
  );
}
