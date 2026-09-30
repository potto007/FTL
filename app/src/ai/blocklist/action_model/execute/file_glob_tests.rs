use std::path::PathBuf;

#[cfg(unix)]
use super::super::search_command_test_util::assert_bash_arguments;
#[cfg(windows)]
use super::super::search_command_test_util::assert_powershell_arguments;
use super::*;
use crate::terminal::shell::ShellType;

#[test]
fn build_find_command_single_quotes_patterns_and_path() {
    let patterns = vec![
        "$(touch /tmp/warp-poc)*.rs".to_string(),
        "owner's*.rs".to_string(),
    ];

    let command = build_find_command(&patterns, "/tmp/repo path", ShellType::Bash);

    assert_eq!(
        command,
        r#"find '/tmp/repo path' -type f -name '$(touch /tmp/warp-poc)*.rs' -o -name 'owner'"'"'s*.rs'"#
    );
}

#[test]
fn build_git_ls_files_command_single_quotes_joined_patterns() {
    let pattern = "$(touch /tmp/warp-poc)*.rs";
    let patterns = vec![pattern.to_string()];
    let target_path = PathBuf::from(std::path::MAIN_SEPARATOR_STR)
        .join("tmp")
        .join("repo");

    let command = build_git_ls_files_command(
        &patterns,
        target_path.to_str().unwrap(),
        None,
        ShellType::Bash,
    );

    let expected = format!(
        "git ls-files -c -o --exclude-standard -- '{}' '{}'",
        target_path.join(pattern).display(),
        target_path.join("*").join(pattern).display(),
    );
    assert_eq!(command, expected);
}

#[test]
fn build_powershell_get_childitem_command_single_quotes_patterns_and_path() {
    let patterns = vec![
        r#"$(New-Item C:\pwn)*.rs"#.to_string(),
        "owner's*.rs".to_string(),
    ];

    let command = build_powershell_get_childitem_command(&patterns, r#"C:\repo path"#);

    assert_eq!(
        command,
        r#"Get-ChildItem -File -Recurse -Include '$(New-Item C:\pwn)*.rs','owner''s*.rs' -Path 'C:\repo path' | ForEach-Object { $_.FullName }"#
    );
}

#[cfg(unix)]
#[test]
fn file_glob_commands_keep_hostile_arguments_literal_in_bash() {
    let patterns = vec![
        "$(: > injection-marker)*.rs".to_string(),
        "`: > injection-marker`*.rs".to_string(),
        "'; : > injection-marker; #".to_string(),
        "line one\nline two\\*.rs".to_string(),
    ];
    let path = "/repo $(: > injection-marker)/owner's";
    let joined: Vec<_> = patterns
        .iter()
        .flat_map(|pattern| {
            [
                join_paths(&[path, pattern], None),
                join_paths(&[path, "*", pattern], None),
            ]
        })
        .collect();
    let mut expected = vec!["ls-files", "-c", "-o", "--exclude-standard", "--"];
    expected.extend(joined.iter().map(String::as_str));
    assert_bash_arguments(
        &build_git_ls_files_command(&patterns, path, None, ShellType::Bash),
        &expected,
    );
    let mut expected = vec![path, "-type", "f"];
    for (index, pattern) in patterns.iter().enumerate() {
        if index > 0 {
            expected.push("-o");
        }
        expected.extend(["-name", pattern.as_str()]);
    }
    assert_bash_arguments(
        &build_find_command(&patterns, path, ShellType::Bash),
        &expected,
    );
}

#[cfg(windows)]
#[test]
fn file_glob_commands_keep_hostile_arguments_literal_in_powershell() {
    let patterns = vec![
        "$(New-Item injection-marker)*.rs".to_string(),
        "'; New-Item injection-marker; #".to_string(),
        "line one\nline two`*.rs".to_string(),
    ];
    let path = "C:\\repo $(New-Item injection-marker)\\owner's";
    let joined: Vec<_> = patterns
        .iter()
        .flat_map(|pattern| {
            [
                join_paths(&[path, pattern], None),
                join_paths(&[path, "*", pattern], None),
            ]
        })
        .collect();
    // PowerShell 调用函数时会消费 --；原生命令的分隔符由快照测试覆盖。
    let mut expected = vec!["ls-files", "-c", "-o", "--exclude-standard"];
    expected.extend(joined.iter().map(String::as_str));
    assert_powershell_arguments(
        &build_git_ls_files_command(&patterns, path, None, ShellType::PowerShell),
        &expected,
    );
    let mut expected = vec![path];
    expected.extend(patterns.iter().map(String::as_str));
    assert_powershell_arguments(
        &build_powershell_get_childitem_command(&patterns, path),
        &expected,
    );
}
