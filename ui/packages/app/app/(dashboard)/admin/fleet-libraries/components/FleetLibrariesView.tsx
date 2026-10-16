import type { PlatformEntry } from "@/lib/api/platform-library-types";
import { PageHeader, PageLayout, PageTitle, Section, SectionHeader } from "@agentsfleet/design-system";
import { FLEET_CATALOG_SECTION, FLEET_LIBRARIES_DESCRIPTION, FLEET_LIBRARY_TITLE } from "../library-copy";
import PlatformCatalogTable from "./PlatformCatalogTable";

export default function FleetLibrariesView({entries}: {entries: PlatformEntry[]}) {
  return <PageLayout fullHeight className="h-full overflow-hidden">
    <PageHeader description={FLEET_LIBRARIES_DESCRIPTION}><PageTitle>{FLEET_LIBRARY_TITLE}</PageTitle></PageHeader>
    <Section aria-label="Platform fleet catalog" className="flex min-h-0 flex-col gap-xl">
      <SectionHeader as="p">{FLEET_CATALOG_SECTION}</SectionHeader>
      <PlatformCatalogTable entries={entries} />
    </Section>
  </PageLayout>;
}
