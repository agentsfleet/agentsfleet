import { spawn } from "node:child_process";

const K_IGNORE = "ignore";
const K_XDG_OPEN = "xdg-open";
const K_WSLVIEW = "wslview";
const K_OPEN = "open";
const K_ERROR = "error";

const K_CMD = "cmd";

function browserDisabled(env) {
  const raw = env.BROWSER;
  if (raw == null) return false;
  const normalized = String(raw).trim().toLowerCase();
  return normalized === "false" || normalized === "0" || normalized === "off" || normalized === "none";
}

function hasDisplay(env) {
  return Boolean(env.DISPLAY || env.WAYLAND_DISPLAY);
}

function isSsh(env) {
  return Boolean(env.SSH_CLIENT || env.SSH_TTY || env.SSH_CONNECTION);
}

function looksLikeWsl(env) {
  const release = `${env.WSL_DISTRO_NAME || ""}${env.WSL_INTEROP || ""}${env.OSTYPE || ""}`.toLowerCase();
  return release.includes("wsl");
}

function commandExists(command) {
  return new Promise((resolve) => {
    const probe = spawn("sh", ["-lc", `command -v ${command} >/dev/null 2>&1`], {
      stdio: K_IGNORE,
    });
    probe.on("exit", (code) => resolve(code === 0));
    probe.on(K_ERROR, () => resolve(false));
  });
}

export async function resolveBrowserCommand(env = process.env, platform = process.platform) {
  if (browserDisabled(env)) {
    return { argv: null, reason: "browser-disabled" };
  }

  if (platform === "win32") {
    return { argv: [K_CMD, "/c", "start", ""], quoteUrl: true, command: K_CMD };
  }

  if (platform === "darwin") {
    return { argv: [K_OPEN], quoteUrl: false, command: K_OPEN };
  }

  if (platform === "linux") {
    const wsl = looksLikeWsl(env);
    if (wsl) {
      if (await commandExists(K_WSLVIEW)) {
        return { argv: [K_WSLVIEW], quoteUrl: false, command: K_WSLVIEW };
      }
      if (!hasDisplay(env)) {
        return { argv: null, reason: "wsl-no-wslview" };
      }
    }

    if (!hasDisplay(env)) {
      return { argv: null, reason: isSsh(env) ? "ssh-no-display" : "no-display" };
    }

    if (await commandExists(K_XDG_OPEN)) {
      return { argv: [K_XDG_OPEN], quoteUrl: false, command: K_XDG_OPEN };
    }

    return { argv: null, reason: "missing-xdg-open" };
  }

  return { argv: null, reason: "unsupported-platform" };
}

export async function openUrl(url, opts = {}) {
  const env = opts.env || process.env;
  const platform = opts.platform || process.platform;

  const resolved = await resolveBrowserCommand(env, platform);
  if (!resolved.argv) return false;

  return new Promise((resolve) => {
    const argv = [...resolved.argv];
    if (resolved.quoteUrl) {
      argv.push(`"${url}"`);
    } else {
      argv.push(url);
    }

    const child = spawn(argv[0], argv.slice(1), {
      detached: true,
      stdio: K_IGNORE,
      windowsVerbatimArguments: resolved.quoteUrl === true,
    });

    child.on(K_ERROR, () => resolve(false));
    child.unref();
    resolve(true);
  });
}
