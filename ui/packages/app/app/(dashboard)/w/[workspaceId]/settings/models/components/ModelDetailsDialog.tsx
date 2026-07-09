"use client";

import {
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
import { providerLabel } from "@/lib/api/model_caps";
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
        <DialogTitle>{target.model_id}</DialogTitle>
        <DialogDescription>Model entry details.</DialogDescription>
      </DialogHeader>
      <DescriptionList>
        <div>
          <DescriptionTerm>Provider</DescriptionTerm>
          <DescriptionDetails>{target.provider ? providerLabel(target.provider) : "Unknown"}</DescriptionDetails>
        </div>
        <div>
          <DescriptionTerm>Kind</DescriptionTerm>
          <DescriptionDetails>{target.kind}</DescriptionDetails>
        </div>
        {target.base_url ? (
          <div>
            <DescriptionTerm>Endpoint</DescriptionTerm>
            <DescriptionDetails mono>{target.base_url}</DescriptionDetails>
          </div>
        ) : null}
        <div>
          <DescriptionTerm>Model</DescriptionTerm>
          <DescriptionDetails mono>{target.model_id}</DescriptionDetails>
        </div>
        <div>
          <DescriptionTerm>Name</DescriptionTerm>
          <DescriptionDetails mono>{target.secret_ref}</DescriptionDetails>
        </div>
        <div>
          <DescriptionTerm>Has key</DescriptionTerm>
          <DescriptionDetails>{target.has_key ? "Yes" : "No — keyless endpoint"}</DescriptionDetails>
        </div>
        <div>
          <DescriptionTerm>Created</DescriptionTerm>
          <DescriptionDetails><Time value={new Date(target.created_at)} /></DescriptionDetails>
        </div>
      </DescriptionList>
    </>
  );
}

/** Read-only detail view for one registry row — never the key material itself. */
export default function ModelDetailsDialog({ target, onOpenChange }: Props) {
  return (
    <Dialog open={target !== null} onOpenChange={onOpenChange}>
      <DialogContent>{target ? <Details target={target} /> : null}</DialogContent>
    </Dialog>
  );
}
