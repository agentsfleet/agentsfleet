// Every string this page renders, named once (RULE UFS). The page and its
// loading state share the title and description, so a change to either shows
// up in both rather than flashing one then the other.
export const LIBRARY_PAGE_TITLE = "Fleet library";
export const LIBRARY_PAGE_DESCRIPTION =
  "Fleet libraries this workspace onboarded. Removing one leaves fleets installed from it running.";
export const LIBRARY_SECTION_LABEL = "Onboarded Fleet libraries";

// The empty state names the command that fills it. An empty list with no next
// step reads as a broken screen rather than an empty one.
export const LIBRARY_EMPTY_TITLE = "Nothing onboarded yet";
export const LIBRARY_EMPTY_BODY =
  "Onboard a Fleet library with: agentsfleet library create --github <owner/repo>";

// The removal confirmation. It names the entry, says what survives, and says
// what does not — someone reading it should not have to guess whether a
// running fleet is about to stop.
export const REMOVE_DIALOG_TITLE = (name: string) => `Remove "${name}" from this workspace?`;
export const REMOVE_DIALOG_BODY =
  "Fleets already installed from it keep running and keep their own copy. " +
  "It leaves the install gallery, and this cannot be undone — onboard it again to get it back.";
export const REMOVE_CONFIRM_LABEL = "Remove";
// The row action is a glyph, and `IconAction` folds this string into both the
// tooltip body and the accessible name — one string, so the two cannot drift.
export const REMOVE_ROW_LABEL = "Remove from this workspace";

export const COLUMN_NAME = "Name";
export const COLUMN_SOURCE = "Source";
// "Time", not "Onboarded": the cell is a relative instant and the header names
// what it holds rather than restating the sentence the page description already
// makes (Indy, 2026-09-22).
export const COLUMN_TIME = "Time";
export const COLUMN_ACTIONS = "Actions";

// Load more, and what a failed one says. The label matches the runner wall's,
// because two pages that page differently teach an operator two habits.
export const LOAD_MORE_LABEL = "Load more";
export const LOADING_LABEL = "Loading…";
export const LOAD_MORE_ERROR_ACTION = "load more library entries";
