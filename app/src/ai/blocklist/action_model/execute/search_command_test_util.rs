use command::blocking::Command;

#[cfg(unix)]
pub(super) fn assert_bash_arguments(command: &str, expected: &[&str]) {
    let directory = tempfile::tempdir().unwrap();
    let script = format!(
        r#"git() {{ printf '%s\0' "$@"; }}
grep() {{ printf '%s\0' "$@"; }}
find() {{ printf '%s\0' "$@"; }}
{command}"#
    );
    let output = Command::new("bash")
        .args(["--noprofile", "--norc", "-c", &script])
        .current_dir(directory.path())
        .output()
        .unwrap();
    assert!(
        !directory.path().join("injection-marker").exists(),
        "model-controlled argument executed shell syntax"
    );
    assert!(output.status.success(), "{output:?}");
    let actual = String::from_utf8(output.stdout).unwrap();
    assert_eq!(actual, format!("{}\0", expected.join("\0")));
}

#[cfg(windows)]
pub(super) fn assert_powershell_arguments(command: &str, expected: &[&str]) {
    let directory = tempfile::tempdir().unwrap();
    let script = format!(
        r#"$ErrorActionPreference = 'Stop'
function Write-Arguments($values) {{
    foreach ($value in $values) {{
        [Console]::Write($value)
        [Console]::Write([char]0)
    }}
}}
function git {{ Write-Arguments $args }}
function Get-ChildItem {{
    param([string]$Path, [switch]$Recurse, [switch]$File, [string[]]$Include)
    Write-Arguments @($Path)
    if ($Include) {{ Write-Arguments $Include }}
}}
function Select-String {{
    param([switch]$NoEmphasis, [switch]$CaseSensitive, [string[]]$Pattern)
    Write-Arguments $Pattern
}}
{command}"#
    );
    let output = Command::new("powershell.exe")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &script,
        ])
        .current_dir(directory.path())
        .output()
        .unwrap();
    assert!(
        !directory.path().join("injection-marker").exists(),
        "model-controlled argument executed shell syntax"
    );
    assert!(output.status.success(), "{output:?}");
    let actual = String::from_utf8(output.stdout).unwrap();
    assert_eq!(actual, format!("{}\0", expected.join("\0")));
}
