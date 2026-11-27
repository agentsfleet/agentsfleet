import { z } from "zod";

export type Request = <T>(method: string, path: string, body?: unknown) => Promise<T>;
type Bag = Record<string, string>;

const SecretNames = z.object({secrets: z.array(z.object({name: z.string()}))});

export class LocalBootstrapError extends Error {
  constructor(message: string, readonly status?: number) { super(message); }
}

/** An existing encrypted bag survives repeat setup with or without machine credentials. */
export async function saveSecret(request: Request, workspaceId: string, name: string, data: Bag | (() => Bag)): Promise<void> {
  const path = `/v1/workspaces/${workspaceId}/secrets`;
  const {secrets} = SecretNames.parse(await request<unknown>("GET", path));
  if (!secrets.some(row => row.name === name)) {
    await request("POST", path, {name, data: typeof data === "function" ? data() : data});
  }
}

/** Only the dedicated platform-admin fixture owns the paid search vendor keys. */
export async function configureWebVendors(request: Request, workspaceId: string, configuredWorkspace?: string): Promise<void> {
  if (configuredWorkspace && configuredWorkspace !== workspaceId) return;
  await saveSecret(request, workspaceId, "parallel", () => {
    const key = process.env.PARALLEL_API_KEY;
    if (!key) throw new LocalBootstrapError("machine configuration must supply PARALLEL_API_KEY for initial web search setup");
    return {api_key: key};
  });
  const exa = process.env.EXA_API_KEY;
  if (exa) await saveSecret(request, workspaceId, "exa", {api_key: exa});
}
