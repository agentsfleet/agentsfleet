import { spawn } from "node:child_process";

export interface BrowserResolutionOk {
  argv: string[];
  quoteUrl?: boolean;
  command: string;
  reason?: undefined;
}

export interface BrowserResolutionBlocked {
  argv: null;
  reason: string;
  command?: undefined;
  quoteUrl?: undefined;
}

export type BrowserResolution = BrowserResolutionOk | BrowserResolutionBlocked;

export interface OpenUrlOptions {
  env?: NodeJS.ProcessEnv | undefined;
  platform?: NodeJS.Platform | undefined;
}

function browserDisabled(env: NodeJS.ProcessEnv): boolean {
  const raw = env.BROWSER;
  if (raw == null) return false;
  const normalized = String(raw).trim().toLowerCase();
  return normalized === "false" || normalized === "0" || normalized === "off" || normalized === "none";
}

function hasDisplay(env: NodeJS.ProcessEnv): boolean {
  return Boolean(env.DISPLAY || env.WAYLAND_DISPLAY);
}

function isSsh(env: NodeJS.ProcessEnv): boolean {
  return Boolean(env.SSH_CLIENT || env.SSH_TTY || env.SSH_CONNECTION);
}

function looksLikeWsl(env: NodeJS.ProcessEnv): boolean {
  const release = `${env.WSL_DISTRO_NAME || ""}${env.WSL_INTEROP || ""}${env.OSTYPE || ""}`.toLowerCase();
  return release.includes("wsl");
}

function commandExists(command: string): Promise<boolean> {
  return new Promise((resolve) => {
    const probe = spawn("sh", ["-lc", `command -v ${command} >/dev/null 2>&1`], {
      stdio: "ignore",
    });
    probe.on("exit", (code) => resolve(code === 0));
    probe.on("error", () => resolve(false));
  });
}

export async function resolveBrowserCommand(
  env: NodeJS.ProcessEnv = process.env,
  platform: NodeJS.Platform = process.platform,
): Promise<BrowserResolution> {
  if (browserDisabled(env)) {
    return { argv: null, reason: "browser-disabled" };
  }

  if (platform === "win32") {
    return { argv: ["cmd", "/c", "start", ""], quoteUrl: true, command: "cmd" };
  }

  if (platform === "darwin") {
    return { argv: ["open"], quoteUrl: false, command: "open" };
  }

  if (platform === "linux") {
    const wsl = looksLikeWsl(env);
    if (wsl) {
      if (await commandExists("wslview")) {
        return { argv: ["wslview"], quoteUrl: false, command: "wslview" };
      }
      if (!hasDisplay(env)) {
        return { argv: null, reason: "wsl-no-wslview" };
      }
    }

    if (!hasDisplay(env)) {
      return { argv: null, reason: isSsh(env) ? "ssh-no-display" : "no-display" };
    }

    if (await commandExists("xdg-open")) {
      return { argv: ["xdg-open"], quoteUrl: false, command: "xdg-open" };
    }

    return { argv: null, reason: "missing-xdg-open" };
  }

  return { argv: null, reason: "unsupported-platform" };
}

export async function openUrl(url: string, opts: OpenUrlOptions = {}): Promise<boolean> {
  const env = opts.env || process.env;
  const platform = opts.platform || process.platform;

  const resolved = await resolveBrowserCommand(env, platform);
  if (!resolved.argv) return false;

  return new Promise((resolve) => {
    const argv: string[] = [...resolved.argv];
    if (resolved.quoteUrl) {
      argv.push(`"${url}"`);
    } else {
      argv.push(url);
    }

    const head = argv[0];
    if (!head) {
      resolve(false);
      return;
    }
    const child = spawn(head, argv.slice(1), {
      detached: true,
      stdio: "ignore",
      windowsVerbatimArguments: resolved.quoteUrl === true,
    });

    child.on("error", () => resolve(false));
    child.unref();
    resolve(true);
  });
}
