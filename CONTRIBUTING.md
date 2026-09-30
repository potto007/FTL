# Contributing to FTL

Use [FTL issues](https://github.com/potto007/FTL/issues) for bugs and proposals,
and submit pull requests to this fork. Include the affected platform, build
version, reproduction steps and relevant validation. Redact credentials and
private terminal or model content from logs.

Read [AGENTS.md](AGENTS.md) for repository architecture and conventions. Keep
changes focused, preserve existing user data and compatibility, and run
`cargo check` plus tests appropriate to the changed behavior. Use
`cargo run -p warp --bin ftl` for a local application build. See
[Windows build instructions](FTL_WINDOWS_BUILD.md) for native packaging and
[product identities](docs/ftl-rebrand.md) for retained compatibility names.

FTL does not promise the upstream project's automated review or triage service.
Review and merge decisions belong to this fork's maintainers. Preserve the
original copyright and license notices when adapting upstream work. Report
security issues according to [SECURITY.md](SECURITY.md).
