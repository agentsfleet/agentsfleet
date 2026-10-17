export const LIBRARY_UNAVAILABLE_TITLE = "Fleet library is unavailable right now";

export function libraryUnavailableDescription(status?: number, selected = false): string {
  if (status === 401) return "Your session has expired. Sign in again to continue.";
  if (status === 403) return "Your account doesn’t have access to this library. Ask your administrator for access.";
  if (selected && (status === 404 || status === 410)) {
    return "This published setup is no longer available, or your access has changed. Choose another setup.";
  }
  return "We couldn’t load the library. Please try again shortly.";
}
