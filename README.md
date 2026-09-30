<div align="center">

<img src="assets/ftl-logo.svg" alt="FTL" width="128" />

# FTL

[简体中文](./README.zh-CN.md) · [日本語](./README.ja.md)

<sub><i>Currently based on <a href="https://github.com/warpdotdev/warp">Warp</a>; evolving independently going forward.</i></sub>

</div>

FTL is an open, local-first terminal with first-class AI and agent support. Plug in any AI provider, bring in any CLI agent, manage SSH hosts inside the terminal — with keys, history and agent state staying on your machine by default.

## What FTL adds over upstream Warp

- **No mandatory cloud** — no account, login, Drive sync or cloud agent history required.
- **BYOP AI providers** — any OpenAI-compatible endpoint, plus native OpenAI / Anthropic / Gemini / DeepSeek / Ollama protocols. Keys stay local.
- **Third-party CLI agents** — DeepSeek-TUI / Codex CLI / Claude Code / Google Antigravity (`agy`) wired into Blocks and the notification center.
- **Built-in SSH host manager** — manage hosts, configs and sessions inside the terminal, with tmux integration.
- **Editable system prompts** — minijinja templates rendered on the client.
- **Rendering fixes** — tuned Markdown pipeline; CJK soft-wrap caret and bold subpixel fixes.
- **Localized UI** — English / Simplified Chinese / Japanese / Russian out of the box, community-extensible.
- **Privacy defaults** — Cloud Agent / Computer Use / Referral / telemetry off by default.

## Existing Zap, OpenWarp or Warp installations

FTL is a fork of Zap, formerly OpenWarp. See [FTL profile compatibility](docs/ftl-rebrand.md)
for product names and existing-data handling. The older
[OpenWarp/Warp migration guide](docs/migrate-from-warp.md) is retained for historical reference.

The independent local agent service, bounded teams, native Windows desktop broker
and multimodal MCP support are described in [agent capabilities](docs/ftl-agent-capabilities.md).

## Roadmap

See [docs/roadmap.md](docs/roadmap.md).

## Acknowledgements

- [Zap](https://github.com/zerx-lab/zap) — the immediate upstream fork and its contributors.
- [Warp](https://github.com/warpdotdev/warp) — the upstream terminal FTL is built on.
- [DeepSeek-TUI](https://github.com/Hmbown/DeepSeek-TUI) — first-class CLI agent partner.
