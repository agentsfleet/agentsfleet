import { Button } from "@agentsfleet/design-system";

// The "write your own template" docs entry. Kept in a neutral module (not the
// client-only AddTemplateDialog) so Server Components can import the URL + the
// shared link without pulling the dialog's module graph.
export const CREATE_TEMPLATE_DOC_URL =
  "https://docs.agentsfleet.net/fleets/templates#writing-your-own";

// Shared "Create a template" affordance across the install surfaces (Fleets
// empty-state, dashboard gallery empty-state, install picker). One source for
// the label + external-link hardening; the button variant flexes per surface.
export function CreateTemplateDocLink({
  variant = "ghost",
}: {
  variant?: "default" | "ghost";
}) {
  return (
    <Button asChild variant={variant} size="sm">
      <a href={CREATE_TEMPLATE_DOC_URL} target="_blank" rel="noopener noreferrer">
        Create a template
      </a>
    </Button>
  );
}
