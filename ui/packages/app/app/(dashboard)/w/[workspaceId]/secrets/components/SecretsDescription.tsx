import { SECRETS_PAGE_DESCRIPTION } from "../copy";

export function SecretsDescription() {
  return <>{SECRETS_PAGE_DESCRIPTION} <span className="font-semibold text-info">Encrypted</span> at rest.</>;
}
