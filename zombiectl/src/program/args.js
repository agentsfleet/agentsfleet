const DEFAULT_API_URL = "http://localhost:3000";

function normalizeApiUrl(url) {
  return String(url || DEFAULT_API_URL).replace(/\/+$/, "");
}

function splitOption(token) {
  const idx = token.indexOf("=");
  if (idx === -1) return { key: token, value: null };
  return { key: token.slice(0, idx), value: token.slice(idx + 1) };
}

function parseFlags(tokens) {
  const options = {};
  const positionals = [];

  for (let i = 0; i < tokens.length; i += 1) {
    const token = tokens[i];
    if (!token.startsWith("--")) {
      positionals.push(token);
      continue;
    }

    const { key, value } = splitOption(token);
    const normalized = key.slice(2);

    if (value !== null) {
      options[normalized] = value;
      continue;
    }

    const next = tokens[i + 1];
    if (next && !next.startsWith("--")) {
      options[normalized] = next;
      i += 1;
      continue;
    }

    options[normalized] = true;
  }

  return { options, positionals };
}

function parseGlobalArgs(argv, env = process.env) {
  const options = {
    json: false,
    noInput: false,
    noOpen: false,
    help: false,
    version: false,
    api: null,
  };

  const rest = [];
  for (let i = 0; i < argv.length; i += 1) {
    const token = argv[i];
    if (token === "--json") {
      options.json = true;
    } else if (token === "--no-input") {
      options.noInput = true;
    } else if (token === "--no-open") {
      options.noOpen = true;
    } else if (token === "--help" || token === "-h") {
      options.help = true;
    } else if (token === "--version") {
      options.version = true;
    } else if (token === "--api") {
      options.api = argv[i + 1] || null;
      i += 1;
    } else if (token.startsWith("--api=")) {
      options.api = token.slice("--api=".length);
    } else {
      rest.push(token);
    }
  }

  const derived = {
    apiUrl: normalizeApiUrl(options.api || env.ZOMBIE_API_URL || env.API_URL || DEFAULT_API_URL),
    json: options.json,
    noInput: options.noInput,
    noOpen: options.noOpen,
    help: options.help,
    version: options.version,
  };

  return { global: derived, rest };
}

export {
  DEFAULT_API_URL,
  normalizeApiUrl,
  parseFlags,
  parseGlobalArgs,
};
