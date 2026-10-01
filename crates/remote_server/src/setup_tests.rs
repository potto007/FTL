use super::*;

#[test]
fn parse_uname_linux_x86_64() {
    let platform = parse_uname_output("Linux x86_64").unwrap();
    assert_eq!(platform.os, RemoteOs::Linux);
    assert_eq!(platform.arch, RemoteArch::X86_64);
}

#[test]
fn parse_uname_linux_aarch64() {
    let platform = parse_uname_output("Linux aarch64").unwrap();
    assert_eq!(platform.os, RemoteOs::Linux);
    assert_eq!(platform.arch, RemoteArch::Aarch64);
}

#[test]
fn parse_uname_darwin_arm64() {
    let platform = parse_uname_output("Darwin arm64").unwrap();
    assert_eq!(platform.os, RemoteOs::MacOs);
    assert_eq!(platform.arch, RemoteArch::Aarch64);
}

#[test]
fn parse_uname_darwin_x86_64() {
    let platform = parse_uname_output("Darwin x86_64").unwrap();
    assert_eq!(platform.os, RemoteOs::MacOs);
    assert_eq!(platform.arch, RemoteArch::X86_64);
}

#[test]
fn parse_uname_linux_armv8l() {
    let platform = parse_uname_output("Linux armv8l").unwrap();
    assert_eq!(platform.os, RemoteOs::Linux);
    assert_eq!(platform.arch, RemoteArch::Aarch64);
}

#[test]
fn parse_uname_skips_shell_initialization_output() {
    let output = "Last login: Mon Apr  7 10:00:00 2025\nWelcome to Ubuntu\nLinux x86_64";
    let platform = parse_uname_output(output).unwrap();
    assert_eq!(platform.os, RemoteOs::Linux);
    assert_eq!(platform.arch, RemoteArch::X86_64);
}

#[test]
fn parse_uname_trims_whitespace() {
    let platform = parse_uname_output("  Linux x86_64  \n").unwrap();
    assert_eq!(platform.os, RemoteOs::Linux);
    assert_eq!(platform.arch, RemoteArch::X86_64);
}

#[test]
fn parse_uname_unsupported_os() {
    let result = parse_uname_output("Windows x86_64");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("unsupported OS"));
}

#[test]
fn parse_uname_unsupported_arch() {
    let result = parse_uname_output("Linux mips");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("unsupported arch"));
}

#[test]
fn parse_uname_empty_output() {
    let result = parse_uname_output("");
    assert!(result.is_err());
}

#[test]
fn parse_uname_missing_arch() {
    let result = parse_uname_output("Linux");
    assert!(result.is_err());
}

#[test]
fn state_is_ready() {
    assert!(RemoteServerSetupState::Ready.is_ready());
    assert!(!RemoteServerSetupState::Checking.is_ready());
    assert!(!RemoteServerSetupState::Initializing.is_ready());
}

#[test]
fn state_is_failed() {
    assert!(RemoteServerSetupState::Failed {
        error: "test".into()
    }
    .is_failed());
    assert!(!RemoteServerSetupState::Ready.is_failed());
}

#[test]
fn state_is_terminal() {
    assert!(RemoteServerSetupState::Ready.is_terminal());
    assert!(RemoteServerSetupState::Failed {
        error: "test".into()
    }
    .is_terminal());
    assert!(RemoteServerSetupState::Unsupported {
        reason: UnsupportedReason::NonGlibc {
            name: "musl".into()
        }
    }
    .is_terminal());
    assert!(!RemoteServerSetupState::Checking.is_terminal());
    assert!(!RemoteServerSetupState::Installing {
        progress_percent: None,
    }
    .is_terminal());
    assert!(!RemoteServerSetupState::Updating.is_terminal());
    assert!(!RemoteServerSetupState::Initializing.is_terminal());
}

#[test]
fn parse_preinstall_supported_glibc() {
    let stdout = "required_glibc=2.31\n\
                  libc_family=glibc\n\
                  libc_version=2.35\n\
                  status=supported\n";
    let result = PreinstallCheckResult::parse(stdout);
    assert_eq!(result.status, PreinstallStatus::Supported);
    assert_eq!(result.libc, RemoteLibc::Glibc(GlibcVersion::new(2, 35)));
    assert!(result.is_supported());
}

#[test]
fn parse_preinstall_legacy_glibc_too_old_now_supported() {
    // remote-server 现在是静态 musl 二进制,旧脚本的 `glibc_too_old` 门禁
    // 已失效。即使老 remote 端缓存着旧脚本并报出该理由,客户端也应把它当
    // 作支持处理,而不是回退到 ControlMaster。
    let stdout = "required_glibc=2.17\n\
                  libc_family=glibc\n\
                  libc_version=2.17\n\
                  status=unsupported\n\
                  reason=glibc_too_old\n";
    let result = PreinstallCheckResult::parse(stdout);
    assert_eq!(result.status, PreinstallStatus::Supported);
    assert!(result.is_supported());
}

#[test]
fn parse_preinstall_legacy_non_glibc_now_supported() {
    // 同理:musl/uclibc 宿主在静态二进制下也能运行。旧脚本的 `non_glibc`
    // 不再触发 fall-back。
    let stdout = "required_glibc=2.17\n\
                  libc_family=musl\n\
                  status=unsupported\n\
                  reason=non_glibc\n";
    let result = PreinstallCheckResult::parse(stdout);
    assert_eq!(result.status, PreinstallStatus::Supported);
    assert_eq!(
        result.libc,
        RemoteLibc::NonGlibc {
            name: "musl".to_string()
        }
    );
    assert!(result.is_supported());
}

#[test]
fn parse_preinstall_musl_host_supported() {
    // 新版脚本在 musl 宿主上直接报 `status=supported`。
    let stdout = "required_glibc=2.17\n\
                  libc_family=musl\n\
                  status=supported\n";
    let result = PreinstallCheckResult::parse(stdout);
    assert_eq!(result.status, PreinstallStatus::Supported);
    assert!(result.is_supported());
}

#[test]
fn parse_preinstall_old_glibc_host_supported() {
    // 新版脚本在旧 glibc(< 2.35)宿主上也直接报 `status=supported`。
    let stdout = "required_glibc=2.17\n\
                  libc_family=glibc\n\
                  libc_version=2.17\n\
                  status=supported\n";
    let result = PreinstallCheckResult::parse(stdout);
    assert_eq!(result.status, PreinstallStatus::Supported);
    assert!(result.is_supported());
}

#[test]
fn parse_preinstall_missing_status_falls_open() {
    // Garbled / partial script output — missing status field. Confirms
    // the fail-open invariant: anything we can't positively classify as
    // unsupported degrades to Unknown and is treated as supported, so a
    // flaky probe doesn't block the install.
    let stdout = "libc_family=glibc\nlibc_version=2.35\n";
    let result = PreinstallCheckResult::parse(stdout);
    assert_eq!(result.status, PreinstallStatus::Unknown);
    assert!(result.is_supported());
}

#[test]
fn oss_remote_server_dir_uses_ftl_namespace() {
    assert_eq!(remote_server_dir(), "~/.ftl/remote-server");
}

#[test]
fn oss_binary_name_matches_ftl_cli() {
    assert_eq!(binary_name(), "ftl");
}

#[test]
fn oss_download_tarball_url_uses_github_release_asset() {
    let platform = RemotePlatform {
        os: RemoteOs::Linux,
        arch: RemoteArch::X86_64,
    };

    let url = download_tarball_url(&platform);

    assert_eq!(
        url,
        "https://github.com/potto007/FTL/releases/latest/download/ftl-linux-x86_64.tar.gz"
    );
    assert!(!url.contains("app.warp.dev"));
    assert!(!url.contains("/download/cli"));
}

#[test]
fn install_script_uses_ftl_asset_and_staging_placeholder() {
    let script = install_script(Some("~/.ftl/remote-server/ftl-upload.tar.gz"));

    assert!(script.contains("staging_tarball_path=\"~/.ftl/remote-server/ftl-upload.tar.gz\""));
    assert!(script.contains("ftl-$os_name-$arch_name.tar.gz"));
    assert!(!script.contains("app.warp.dev"));
    assert!(!script.contains("/download/cli"));
    assert!(script.contains("[ \"false\" = \"true\" ]"));
    assert!(script.contains("bin=\"$tmpdir/ftl\""));
}

#[test]
fn oss_release_urls_are_pinned_to_ftl_and_keep_legacy_channels_separate() {
    assert_eq!(
        download_url_for_channel(Channel::Oss, Some("v1.2.3")),
        "https://github.com/potto007/FTL/releases/download/v1.2.3"
    );
    assert_eq!(artifact_prefix(Channel::Oss), "ftl");
    for channel in [
        Channel::Stable,
        Channel::Preview,
        Channel::Dev,
        Channel::Local,
        Channel::Integration,
    ] {
        assert_eq!(
            download_url_for_channel(channel, None),
            "https://github.com/zerx-lab/warp/releases/latest/download"
        );
        assert_eq!(artifact_prefix(channel), "zap");
    }
}

#[test]
fn published_remote_assets_match_ftl_release_workflow() {
    let workflow = include_str!("../../../.github/workflows/ftl_release.yml");
    assert!(workflow.contains("tar czf ftl-linux-x86_64.tar.gz"));
    assert!(workflow.contains("tar czf ftl-macos-${{ matrix.arch }}.tar.gz"));
    assert!(workflow.contains("- arch: aarch64"));
    assert!(workflow.contains("- arch: x86_64"));
    for (os, arch, expected) in [
        (
            RemoteOs::Linux,
            RemoteArch::X86_64,
            "ftl-linux-x86_64.tar.gz",
        ),
        (
            RemoteOs::MacOs,
            RemoteArch::X86_64,
            "ftl-macos-x86_64.tar.gz",
        ),
        (
            RemoteOs::MacOs,
            RemoteArch::Aarch64,
            "ftl-macos-aarch64.tar.gz",
        ),
    ] {
        assert!(download_tarball_url(&RemotePlatform { os, arch }).ends_with(expected));
    }
}
