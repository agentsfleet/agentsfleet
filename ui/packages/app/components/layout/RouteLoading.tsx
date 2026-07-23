import { PageHeader, PageLayout, PageTitle, Spinner } from "@agentsfleet/design-system";

import { LoadingVerbLabel } from "./LoadingVerbLabel";
import { loadingAccessibleName } from "./loading-verbs";

// Shared route-level loading fallback (Next.js loading.tsx). Paints the page's
// exact title + description instantly so the header never wobbles or flashes a
// wrong title on navigation, with a spinner in the content area for one
// consistent "loading" signal across every dashboard route.
//
// The spinner text carries a per-mount random verb ("Wrangling Fleets…") rather
// than a static "Loading". aria-label pins the announced name to the plain
// wording so the whimsy stays visual — assistive tech reads "Loading Fleets".
export default function RouteLoading({
  title,
  description,
}: {
  title: string;
  description?: string;
}) {
  return (
    <PageLayout>
      <PageHeader description={description}>
        <PageTitle>{title}</PageTitle>
      </PageHeader>
      <Spinner
        size="lg"
        label={<LoadingVerbLabel title={title} />}
        aria-label={loadingAccessibleName(title)}
        className="py-16 text-sm"
      />
    </PageLayout>
  );
}
