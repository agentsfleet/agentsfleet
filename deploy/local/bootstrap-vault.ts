import { z } from "zod";

export type Request = <T>(method: string, path: string, body?: unknown) => Promise<T>;
/** Runs the 1Password command line with these arguments and returns its output. */
export type Op = (args: string[]) => Promise<string>;
type Bag = Record<string, string>;

const SecretNames = z.object({secrets: z.array(z.object({name: z.string()}))});

export class LocalBootstrapError extends Error {
  constructor(message: string, readonly status?: number) { super(message); }
}

/** An existing encrypted bag survives repeat setup with or without machine credentials. */
export async function saveSecret(request: Request, workspaceId: string, name: string, data: Bag | (() => Bag | Promise<Bag>)): Promise<void> {
  const path = `/v1/workspaces/${workspaceId}/secrets`;
  const {secrets} = SecretNames.parse(await request<unknown>("GET", path));
  if (!secrets.some(row => row.name === name)) {
    await request("POST", path, {name, data: typeof data === "function" ? await data() : data});
  }
}

/** Only the dedicated platform-admin fixture owns the paid search vendor keys. */
export async function configureWebVendors(request: Request, workspaceId: string, op: Op, configuredWorkspace?: string): Promise<void> {
  if (configuredWorkspace && configuredWorkspace !== workspaceId) return;
  const vault = process.env.ACCEPTANCE_LOCAL_CONNECTOR_VAULT;
  for (const name of ["parallel", "exa"]) {
    const variable = `${name.toUpperCase()}_API_KEY`;
    const supplied = process.env[variable];
    if (name === "exa" && !supplied && !vault) continue;
    await saveSecret(request, workspaceId, name, async () => {
      const key = supplied || (vault && await op(["read", "--no-newline", `op://${vault}/${name}/credential`]));
      if (!key) throw new LocalBootstrapError(`supply ${variable} or a development vault credential for initial web search setup`);
      return {api_key: key};
    });
  }
}
