import { Suspense } from "react";
import { auth } from "@clerk/nextjs/server";
import { redirect } from "next/navigation";
import { PageHeader, PageLayout, PageTitle, Skeleton } from "@agentsfleet/design-system";
import { listWorkspaceFleetLibraryCached } from "@/lib/api/fleet-library";
import { LIBRARY_ERROR_KIND, type LibraryError } from "@/lib/api/library-types";
import { listSecrets } from "@/lib/api/secrets";
import { InstallFleet } from "./InstallFleet";
import { INSTALL_PAGE_DESCRIPTION, INSTALL_PAGE_TITLE } from "./library-docs";
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

// Gallery-first install. The header paints immediately and the gallery streams
// in beneath it, matching the Approvals/Events/Fleets routes. Previously this
// awaited both reads before rendering a single pixel, so the whole screen was
// gated on whichever of the library and the vault answered last.
export default async function InstallFleetPage({
  params,
  searchParams,
}: {
  params: Promise<{ workspaceId: string }>;
  searchParams: Promise<SearchParams>;
}) {
  const { workspaceId } = await params;
  const { getToken } = await auth();
  const token = await getToken();
  if (!token) redirect("/sign-in");

  const query = await searchParams;

  return (
    <PageLayout>
      <PageHeader description={INSTALL_PAGE_DESCRIPTION}>
        <PageTitle>{INSTALL_PAGE_TITLE}</PageTitle>
      </PageHeader>

      <Suspense fallback={<InstallGallerySkeleton />}>
        <InstallFleetData workspaceId={workspaceId} query={query} />
      </Suspense>
    </PageLayout>
  );
}

/**
 * Stable loading region for the gallery.
 *
 * Fixed height and card count so the shell does not reflow when the real
 * gallery arrives, and no shimmer — `Skeleton` owns motion and honours
 * reduced-motion, which a bespoke animated placeholder here would not.
 */
function InstallGallerySkeleton() {
  return (
    <div className="space-y-sm" aria-busy="true" aria-label="Loading fleet library">
      <Skeleton className="h-5 w-32" />
      <div className="grid grid-cols-1 gap-md sm:grid-cols-2 lg:grid-cols-3">
        <Skeleton className="h-40 rounded-lg" />
        <Skeleton className="h-40 rounded-lg" />
        <Skeleton className="h-40 rounded-lg" />
      </div>
    </div>
  );
}

/**
 * Async data region: reads the first gallery page and the workspace's
 * credential names, then resolves the deep-link selection. Exported so it
 * renders and tests in isolation, matching `ApprovalsData`.
 *
 * Neither read is allowed to REJECT. Suspense here buys latency, not error
 * handling: a rejected promise would throw in render and need an
 * ErrorBoundary, which would trade the failed-versus-empty distinction this
 * workstream exists to draw for an undifferentiated fallback. Both reads
 * resolve to data-or-typed-error instead.
 */
export async function InstallFleetData({
  workspaceId,
  query,
}: {
  workspaceId: string;
  query: SearchParams;
}) {
  const { getToken, sessionClaims } = await auth();
  const token = await getToken();
  if (!token) return null;

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
  );
}
