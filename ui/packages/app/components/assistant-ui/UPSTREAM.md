# assistant-ui component sources

The terminal, ANSI parser, diff and range helpers are adapted from the official assistant-ui shadcn registry. CodeHeader uses its documented Markdown code-header slot and source composition. Snapshot fetched November 11, 2026.

- `https://r.assistant-ui.com/elements-terminal-block.json` — SHA-256 `b027d64cd326b58343b53b979d41c22bf6ed52d84d15245b4c7907e1970fdca0`
- `https://r.assistant-ui.com/elements-code-diff.json` — SHA-256 `8f4a16c8bf93b0f8e42f38eaa9dab84768e39bc93ae93de6e5b1533c3710eac5`
- `https://r.assistant-ui.com/elements-range.json` — SHA-256 `6d3305f7fc3821666fb943d1bcb3a1ac3cae32188c7562ce3590101c6ffd2d1b`
- `https://r.assistant-ui.com/markdown-text.json` — SHA-256 `3f02f9655eb1fea45824e300ca7a40fef0db67fc7f8e2378755f85e403465701`

Local adaptations: shared design-system imports, Manrope/Commit Mono tokens, Phosphor icons, compact spacing, accessible diff side labels, static recorded output and reduced motion. Terminal outcomes retain unknown/interrupted results and never assume exit code zero when it was not recorded. The shared copy control reports clipboard failures.

The existing assistant-ui runtime and tool registration APIs own runtime state. These copied presentation components do not execute tools.

## Upstream license

```text
MIT License

Copyright (c) 2026 AgentbaseAI Inc.

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

```
