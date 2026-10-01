# FTL Windows 构建

本仓库使用 FTL 应用名称、ftl.exe、dev.ftl.FTL 标识及 FTLSetup.exe。旧 Zap 资料原样保留，首次运行仅向独立 FTL 资料导入有效的便携设置，详见 docs/ftl-rebrand.md。

## 共享 Cargo 编译产物

在 x64 Visual Studio 开发者 PowerShell 中设置一个绝对路径,让相关仓库共用 Cargo target 目录:

```powershell
$env:CARGO_TARGET_DIR = 'C:\src\oss\.cargo-target\warp-ftl'
$env:RUSTFLAGS = '-C symbol-mangling-version=v0'
$env:CARGO_BUILD_JOBS = '2'
$env:MAX_JOBS = '2'
.\script\windows\bundle.ps1 -CHANNEL oss -ARCH x64 -RELEASE_TAG 'v0.2026.09.30.00.00.ftl_01'
```

需要 Rust 1.92.0、MSVC x64 工具链、Windows SDK、protoc、libclang、原生 Windows Perl(例如 Strawberry Perl)、cargo-about 和 Inno Setup。设置 PROTOC、LIBCLANG_PATH 及相应 PATH。Windows 必须覆盖仓库通用配置中的 macOS linker flag;保留 symbol-mangling-version=v0 即可。

bundle.ps1 的二进制、schema 生成和安装器输入都遵循 CARGO_TARGET_DIR。安装器生成在 script/windows/Output/FTLSetup.exe。未设置 CARGO_TARGET_DIR 时维持仓库内 target 目录。

所有共用此目录的构建需串行运行,包括安装资源准备阶段。Cargo 自身会锁定编译目录,但打包资源复制不受 Cargo 锁保护。

初次可复制已有可信 target 缓存作为种子。后续让 Cargo 按工具链、目标、profile、features、flags、依赖和源文件 fingerprint 判断复用或重编译,不要手工强制跳过检查。不同工作区不保证全部复用。Cargo 官方也推荐 sccache 用于跨工作区的长期编译缓存;已有普通 target 产物不能直接导入 sccache。

参考:https://doc.rust-lang.org/cargo/reference/build-cache.html

## FTL 更新来源

Windows OSS bundle 默认不启用 autoupdate feature,因此不会后台自动检查更新。关于页面仍可手动检查,但 OSS 的元数据、下载 URL 和失败兜底链接均指向 potto007/FTL。没有可用 Release 时不会回退到上游 FTL 安装器。其他发布 channel 的官方更新路径不变。

本地构建未签名。构建和打包不会自动安装应用。

Windows 安装器同时包含 ftl-agent-service.exe，位于 ftl.exe 旁；不会自动运行或注册为系统服务。
