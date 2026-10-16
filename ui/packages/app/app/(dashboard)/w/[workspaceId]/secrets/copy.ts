export const SECRETS_PAGE_TITLE = "Secrets";
export const SECRETS_PAGE_DESCRIPTION = "Give your fleets access to services without putting credentials in a chat.";
export const SECRET_ROW_DESCRIPTION = "Encrypted and hidden.";
export const CREATE_SECRET_TOOLTIP = "Create an encrypted workspace secret.";

export const DELETE_SECRET_DESCRIPTION = "Deleting breaks fleets that reference it. This cannot be undone.";
export const DELETE_SECRET_CONFIRM = "Delete";
export const DELETE_CONNECTION_CONFIRM = "Delete and disconnect";

/** What deleting a secret that holds a live connection does, in its own words. */
export function deleteConnectionDescription(connection: string): string {
  return `This secret is your ${connection} connection under Integrations > Connected. Deleting it disconnects ${connection}: fleets lose ${connection} access and its events stop arriving until you connect it again. This cannot be undone.`;
}
