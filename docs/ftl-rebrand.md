# FTL product identity and existing profiles

FTL is a fork of [Zap](https://github.com/zerx-lab/zap), which derives from
[Warp](https://github.com/warpdotdev/warp). Copyright, license notices, historical
changelogs and upstream references retain their original attribution.

The product is **FTL**. The application binary and command are `ftl`
(`ftl.exe` on Windows), the application identifier is `dev.ftl.FTL`, and the
Windows installer is `FTLSetup.exe` with Inno AppId `ftl`. The macOS app is
`FTL.app`; Linux packages are `ftl` and `ftl-cli`. Release/download links point
to [potto007/FTL](https://github.com/potto007/FTL). The URL scheme is `ftl://`.
Old Zap installers and sealed build artifacts are not modified by the rebrand.

## Data compatibility

New profiles use these locations:

| Data | Windows | Linux | macOS |
|---|---|---|---|
| User configuration | `%LOCALAPPDATA%\ftl\FTL\config` | `$XDG_CONFIG_HOME/ftl` or `~/.config/ftl` | `~/.ftl` |
| Portable data | `%APPDATA%\ftl\FTL\data` | `$XDG_DATA_HOME/ftl` or `~/.local/share/ftl` | `~/.ftl` |
| State and credentials | `%LOCALAPPDATA%\ftl\FTL\data` | `$XDG_STATE_HOME/ftl` or `~/.local/state/ftl` | `~/Library/Application Support/dev.ftl.FTL` |
| Home skills and MCP config | `%USERPROFILE%\.ftl` | `~/.ftl` | `~/.ftl` |

All platforms use a separate FTL profile. Existing Zap data is left untouched.
Separate databases allow the two products to coexist without concurrent writes
to the same profile.

On first launch, a packaged Windows build copies a valid UTF-8 TOML snapshot of
`%LOCALAPPDATA%\zap\Zap\config\settings.toml` into the FTL config directory
only if FTL has no settings file. Publication never overwrites an existing file;
a completion marker prevents later resets from unexpectedly importing old
settings again. The snapshot is limited to 8 MiB, and invalid input is skipped
without logging its contents. This preserves provider definitions and the
selected local model, including the configured local Qwen endpoint.

This is a portable-settings import, not a complete profile migration. Databases,
conversation history, keychains, DPAPI credential stores, private registry
preferences, custom theme assets and other files are not copied. Existing API
keys may need to be entered again; local providers without keys keep working.
Use an explicit offline export/import for additional portable data with both
apps closed. FTL does not delete old data or uninstall Zap. New Windows
preferences use `HKCU\Software\FTL\FTL`.

Repository `.warp` configuration, `WARP_*` shell/build interfaces, protocol and
serialization identifiers, and upstream Rust `warp*` library names remain for
compatibility. SSH keychain service `zap.ssh` and the Linux secure-storage
derivation label remain unchanged so existing encrypted secrets stay readable.
Sync writes new `FTL_CONFIG` / `ftl_config.json` records and also recognizes old
`ZAP_CONFIG` / `zap_config.json` records. Existing sync is opt-in as before.

The original OpenWarp/Warp migration guides are retained as historical guides
to Zap; they are not instructions to copy a live database or key store into FTL.

## Build and site configuration

`cargo run -p warp --bin ftl` runs the app; the internal app package remains
`warp` to avoid renaming its large upstream library API. The local SFTP/sync
packages are now `ftl_sftp` and `ftl_sync`. The release workflow is
`.github/workflows/ftl_release.yml`. See [Windows build notes](../FTL_WINDOWS_BUILD.md)
and [agent capabilities](ftl-agent-capabilities.md).

The website is source only until explicitly deployed. Set `FTL_SITE_URL` to its
actual deployed origin; the default is localhost. Analytics has no default
destination and requires both `PUBLIC_FTL_ANALYTICS_URL` and
`PUBLIC_FTL_ANALYTICS_CLIENT_ID`. Workflow deployment also requires the explicit
`FTL_DEPLOY_WEBSITE` repository variable. No former-maintainer analytics account
or website deployment is implicitly inherited.
