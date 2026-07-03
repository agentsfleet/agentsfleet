import { Button } from "@agentsfleet/design-system";

// The "write your own template" docs entry. Kept in a neutral module (not the
// client-only AddTemplateDialog) so Server Components can import the URL + the
// shared link without pulling the dialog's module graph.
export const CREATE_TEMPLATE_DOC_URL =
  "https://docs.agentsfleet.net/fleets/templates#writing-your-own";

// Shared copy for the template-gallery empty state (dashboard embed + install
// picker) so the two surfaces can never drift apart.
export const TEMPLATES_EMPTY_TITLE = "No templates found";
export const TEMPLATES_EMPTY_DESCRIPTION =
  "Write your own template to install your first fleet.";

// Shared "Learn more" docs affordance across the install surfaces (dashboard
// gallery empty-state, install picker). Secondary by design — the primary CTA
// beside it is always the concrete action (Install fleet / Create a template).
export function TemplateDocsLink() {
  return (
    <Button asChild variant="outline" size="sm">
      <a href={CREATE_TEMPLATE_DOC_URL} target="_blank" rel="noopener noreferrer">
        Learn more
      </a>
    </Button>
  );
}
