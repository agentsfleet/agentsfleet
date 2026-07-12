import { runFleetctl } from "./cli.js";

interface SeedResult {
  readonly code: number;
  readonly stdout: string;
  readonly stderr: string;
}

type SecretRunner = (
  args: ReadonlyArray<string>,
  opts: { readonly env: Readonly<Record<string, string>> },
) => Promise<SeedResult>;

const PLATFORM_SECRET_PAYLOADS = {
  fly: { host: "https://api.fly.io", api_token: "acceptance-placeholder" },
  upstash: { host: "https://api.upstash.com", api_token: "acceptance-placeholder" },
  slack: { host: "https://slack.com/api", bot_token: "acceptance-placeholder" },
  github: { webhook_secret: "acceptance-placeholder", api_token: "acceptance-placeholder" },
} as const;

export async function ensurePlatformSecretsSeeded(
  env: Readonly<Record<string, string>>,
  run: SecretRunner = runFleetctl,
): Promise<void> {
  for (const [name, data] of Object.entries(PLATFORM_SECRET_PAYLOADS)) {
    const result = await run(
      ["secret", "create", name, "--data", JSON.stringify(data), "--json"],
      { env },
    );
    if (result.code !== 0) {
      throw new Error(`platform secret seed failed for ${name}: ${result.stderr.trim() || result.stdout.trim()}`);
    }
  }
}
