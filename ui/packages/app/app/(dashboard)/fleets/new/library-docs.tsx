import { Button } from "@agentsfleet/design-system";

// The "write your own template" docs entry. Kept in a neutral module (not the
// client-only AddLibraryDialog) so Server Components can import the URL + the
// shared link without pulling the dialog's module graph.
export const CREATE_LIBRARY_DOC_URL =
  "https://docs.agentsfleet.net/fleets/library#writing-your-own";

// Shared copy for the Fleet library empty state (dashboard embed + install
// picker) so the two surfaces can never drift apart.
export const FLEET_LIBRARY_EMPTY_TITLE = "No prebuilt fleet library found";
// Only shown alongside the Create-fleet-library action — a viewer without
// library:write never sees an invitation to do something they can't.
export const FLEET_LIBRARY_EMPTY_DESCRIPTION = "Write your own fleet library.";
export const FLEET_LIBRARY_EMPTY_DESCRIPTION_READONLY =
  "Ask a workspace admin to add one.";

// Shared "Learn more" docs affordance across the install surfaces (dashboard
// gallery empty-state, install picker). Secondary by design — the primary CTA
// beside it is always the concrete action (Install fleet / Create fleet library).
export function LibraryDocsLink() {
  return (
    <Button asChild variant="outline" size="sm">
      <a href={CREATE_LIBRARY_DOC_URL} target="_blank" rel="noopener noreferrer">
        Learn more
      </a>
    </Button>
  );
}
