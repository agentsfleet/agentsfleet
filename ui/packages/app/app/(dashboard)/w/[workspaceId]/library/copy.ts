export const LIBRARY_PAGE_TITLE = "Fleet library";
export const LIBRARY_PAGE_DESCRIPTION = "Start with a published setup. Make it yours in chat.";

// The removal confirmation. It names the entry, says what survives, and says
// what does not — someone reading it should not have to guess whether a
// running fleet is about to stop.
export const REMOVE_DIALOG_TITLE = (name: string) => `Remove "${name}" from this workspace?`;
export const REMOVE_DIALOG_BODY =
  "Fleets already installed from it keep running and keep their own copy. " +
  "It leaves the install gallery, and this cannot be undone — publish a new entry from chat to share it again.";
export const REMOVE_CONFIRM_LABEL = "Remove";
// The row action is a glyph, and `IconAction` folds this string into both the
// tooltip body and the accessible name — one string, so the two cannot drift.
export const REMOVE_ROW_LABEL = "Remove from this workspace";
