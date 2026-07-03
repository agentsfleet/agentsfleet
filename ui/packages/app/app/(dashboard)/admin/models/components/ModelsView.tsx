"use client";

import { useState } from "react";
import { PageHeader, PageTitle, Section } from "@agentsfleet/design-system";
import type { AdminModel, AdminModelList } from "@/lib/api/admin_models";
import CatalogueList from "./CatalogueList";
import AddModelDialog from "./AddModelDialog";
import PlatformDefaultCard from "./PlatformDefaultCard";

// Single source of truth for the catalogue: the table renders it, the Add dialog
// appends to it, deletes remove from it, and the Platform Default card reads it
// for its model picker — so adding a model immediately makes it selectable as the
// default without a round-trip.
export default function ModelsView({ initial }: { initial: AdminModelList }) {
  const [models, setModels] = useState<AdminModel[]>(initial.models);

  return (
    <div>
      <PageHeader>
        <PageTitle>Models</PageTitle>
        <AddModelDialog onCreated={(m) => setModels((prev) => [...prev, m])} />
      </PageHeader>
      <p className="mb-6 max-w-2xl text-sm text-muted-foreground">
        Every model your team can run, priced per token. The platform default below runs for users
        who don&apos;t bring their own key.
      </p>

      <Section asChild>
        <section aria-label="Model catalogue">
          <CatalogueList
            models={models}
            onDeleted={(uid) => setModels((prev) => prev.filter((m) => m.uid !== uid))}
          />
        </section>
      </Section>

      <Section asChild>
        <section aria-label="Platform default" className="mt-10">
          <PlatformDefaultCard models={models} />
        </section>
      </Section>
    </div>
  );
}
