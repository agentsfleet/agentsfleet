import { PageHeader, PageTitle, Spinner } from "@agentsfleet/design-system";

// Shared route-level loading fallback (Next.js loading.tsx). Paints the page's
// exact title + description instantly so the header never wobbles or flashes a
// wrong title on navigation, with a spinner in the content area for one
// consistent "loading" signal across every dashboard route.
export default function RouteLoading({
  title,
  description,
}: {
  title: string;
  description?: string;
}) {
  return (
    <div>
      <PageHeader description={description}>
        <PageTitle>{title}</PageTitle>
      </PageHeader>
      <Spinner size="lg" label={`Loading ${title}…`} className="py-16 text-sm" />
    </div>
  );
}
