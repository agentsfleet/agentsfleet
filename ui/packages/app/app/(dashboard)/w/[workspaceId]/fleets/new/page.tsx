import { auth } from "@clerk/nextjs/server";
import { redirect } from "next/navigation";
import { PageHeader, PageLayout, PageTitle } from "@agentsfleet/design-system";
import { listWorkspaceFleetLibraryCached } from "@/lib/api/fleet-library";
import { LIBRARY_ERROR_KIND, type LibraryError } from "@/lib/api/library-types";
import { listSecrets } from "@/lib/api/secrets";
import { InstallFleet } from "./InstallFleet";
import { hasLibraryWriteScope } from "../scope";

export const dynamic = "force-dynamic";

type SearchParams = {
  library_visibility?: string | string[];
  library_id?: string | string[];
  /** Cursor of the page the link was built from, so a shared link lands on it. */
  library_after?: string | string[];
  create?: string | string[];
};

/** First value of a possibly-repeated query parameter, or undefined. */
function one(value: string | string[] | undefined): string | undefined {
  if (Array.isArray(value)) return value[0];
  return value;
}
const INSTALL_PAGE_DESCRIPTION = "Start a fleet from the library. Watch it run in a loop.";

// Gallery-first install. Library entries + the workspace's existing
// credential names are fetched server-side (workspace from the URL) so the
// client orchestrator can render the gallery and the credential preview
// without a client round-trip.
export default async function InstallFleetPage({
  params,
  searchParams,
}: {
  params: Promise<{ workspaceId: string }>;
  searchParams: Promise<SearchParams>;
}) {
  const { workspaceId } = await params;
  const { getToken, sessionClaims } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  const query = await searchParams;
  // An unparseable cursor is discarded in favour of the first page rather than
  // surfacing an error — a bad link should still land somewhere useful.
  const after = one(query.library_after) ?? null;

  // A failed gallery read is a failure, not an empty library. The previous
  // `.catch(() => [])` told a workspace its library was empty when the read
  // merely failed, offering no retry and no way to tell the two apart.
  const galleryRead = listWorkspaceFleetLibraryCached(workspaceId, token, after).then(
    (result) => ({ result, error: null }),
    (cause: unknown) => ({
      result: null,
      error: {
        kind: LIBRARY_ERROR_KIND.unknown,
        detail: cause instanceof Error ? cause.message : undefined,
      } satisfies LibraryError,
    }),
  );

  const [gallery, credentialNames] = await Promise.all([
    galleryRead,
    listSecrets(workspaceId, token)
      .then((response) => response.secrets.map((secret) => secret.name))
      // null (not []) when the vault read fails: the preview must not mistake an
      // unreadable vault for an empty one and falsely gate create.
      .catch(() => null),
  ]);
  const { result: pageResult, error: pageError } = gallery;

  // Deep-link selection is resolved HERE, on the server, against the page that
  // was just read. Resolving it in a client effect made the gallery paint
  // first and the confirm step replace it a frame later — the flash this
  // removes. An id that is not on the loaded page yields a not-found selection
  // state rather than an error: it neither enumerates nor breaks the page.
  const requestedId = one(query.library_id);
  const requestedVisibility = one(query.library_visibility);
  const initialSelection =
    requestedId && pageResult
      ? (pageResult.items.find(
          (entry) =>
            entry.id === requestedId &&
            (requestedVisibility === undefined || entry.visibility === requestedVisibility),
        ) ?? null)
      : null;
  const selectionNotFound = Boolean(requestedId) && initialSelection === null;

  // ?create=1 (the dashboard empty-state CTA) opens the add-library-entry dialog
  // immediately — no second identical empty state between click and form.
  const initialCreateOpen = one(query.create) === "1";

  return (
    <PageLayout>
      <PageHeader description={INSTALL_PAGE_DESCRIPTION}>
        <PageTitle>Install fleet</PageTitle>
      </PageHeader>
      <InstallFleet
        workspaceId={workspaceId}
        initialPage={pageResult}
        initialError={pageError}
        initialSelection={initialSelection}
        selectionNotFound={selectionNotFound}
        presentCredentialNames={credentialNames}
        canAddLibraryEntry={hasLibraryWriteScope(sessionClaims)}
        initialCreateOpen={initialCreateOpen}
      />
    </PageLayout>
  );
}
