import { spawn } from "node:child_process";

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
      stdio: "ignore",
    });
    probe.on("exit", (code) => resolve(code === 0));
    probe.on("error", () => resolve(false));
  });
}

export async function resolveBrowserCommand(env = process.env, platform = process.platform) {
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
      stdio: "ignore",
      windowsVerbatimArguments: resolved.quoteUrl === true,
    });

    child.on("error", () => resolve(false));
    child.unref();
    resolve(true);
  });
}
