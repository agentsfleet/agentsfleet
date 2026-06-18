import { spawn } from "node:child_process";

export interface BrowserResolutionOk {
  argv: [string, ...string[]];
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
  // Injectable spawner. Defaults to node:child_process spawn; tests pass a
  // stub so the spawn path is exercised WITHOUT shelling out to the OS opener
  // (`open <url>` on macOS launches a real browser tab otherwise). Mirrors the
  // env/platform injection above and the fetchImpl/sleepImpl seams elsewhere.
  spawnImpl?: typeof spawn | undefined;
}

export type BrowserCommandExists = (command: string) => Promise<boolean>;

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

// Async (non-blocking) probe so the event loop is never stalled while we ask
// the shell whether an opener binary exists. `spawnImpl` is injectable for the
// same reason as openUrl below — tests drive the close/error paths through a
// stub instead of shelling out to the real OS. Exported so those paths are
// directly coverable.
export function commandExists(
  command: string,
  spawnImpl: typeof spawn = spawn,
): Promise<boolean> {
  return new Promise((resolve) => {
    const probe = spawnImpl("sh", ["-lc", `command -v ${command} >/dev/null 2>&1`], {
      stdio: STDIO_IGNORE,
    });
    probe.on(STATUS_ERROR, () => resolve(false));
    probe.on(STATUS_CLOSE, (code) => resolve(code === 0));
  });
}

export async function resolveBrowserCommand(
  env: NodeJS.ProcessEnv = process.env,
  platform: NodeJS.Platform = process.platform,
  commandExistsImpl: BrowserCommandExists = commandExists,
): Promise<BrowserResolution> {
  if (browserDisabled(env)) {
    return { argv: null, reason: "browser-disabled" };
  }

  if (platform === "win32") {
    return { argv: [WINDOWS_CMD_COMMAND, "/c", "start", ""], quoteUrl: true, command: WINDOWS_CMD_COMMAND };
  }

  if (platform === "darwin") {
    return { argv: [MAC_OPEN_COMMAND], quoteUrl: false, command: MAC_OPEN_COMMAND };
  }

  if (platform === "linux") {
    const wsl = looksLikeWsl(env);
    if (wsl) {
      if (await commandExistsImpl(WSLVIEW_COMMAND)) {
        return { argv: [WSLVIEW_COMMAND], quoteUrl: false, command: WSLVIEW_COMMAND };
      }
      if (!hasDisplay(env)) {
        return { argv: null, reason: "wsl-no-wslview" };
      }
    }

    if (!hasDisplay(env)) {
      return { argv: null, reason: isSsh(env) ? "ssh-no-display" : "no-display" };
    }

    if (await commandExistsImpl(XDG_OPEN_COMMAND)) {
      return { argv: [XDG_OPEN_COMMAND], quoteUrl: false, command: XDG_OPEN_COMMAND };
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
    const [head, ...argv] = resolved.argv;
    if (resolved.quoteUrl) {
      argv.push(`"${url}"`);
    } else {
      argv.push(url);
    }

    const doSpawn = opts.spawnImpl ?? spawn;
    const child = doSpawn(head, argv, {
      detached: true,
      stdio: STDIO_IGNORE,
      windowsVerbatimArguments: resolved.quoteUrl === true,
    });

    child.on(STATUS_ERROR, () => resolve(false));
    child.unref();
    resolve(true);
  });
}
const WINDOWS_CMD_COMMAND = "cmd" as const;
const STATUS_ERROR = "error" as const;
const STATUS_CLOSE = "close" as const;
const STDIO_IGNORE = "ignore" as const;
const MAC_OPEN_COMMAND = "open" as const;
const WSLVIEW_COMMAND = "wslview" as const;
const XDG_OPEN_COMMAND = "xdg-open" as const;
