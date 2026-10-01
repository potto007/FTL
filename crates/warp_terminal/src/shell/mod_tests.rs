use super::*;

#[test]
fn test_parse_history_bash() {
    let history_lines = "cat ~/.bash_history
#1618089175
ls
#1618089176
pwd";
    assert_eq!(
        ShellType::Bash.parse_history(history_lines.as_bytes()),
        vec![
            "cat ~/.bash_history".to_string(),
            "ls".to_string(),
            "pwd".to_string()
        ]
    );
}

#[test]
fn test_strip_zsh_extended_prefix() {
    // Extended history format.
    assert_eq!(
        strip_zsh_extended_prefix(": 1699251735:0;an extended history command"),
        "an extended history command"
    );

    // Non-extended format (no prefix to strip).
    assert_eq!(
        strip_zsh_extended_prefix("cat ~/.zsh_history"),
        "cat ~/.zsh_history"
    );

    // Edge cases that should NOT be stripped.
    assert_eq!(
        strip_zsh_extended_prefix(": not_a_timestamp"),
        ": not_a_timestamp"
    );
    assert_eq!(strip_zsh_extended_prefix(": 123;"), ": 123;"); // Missing second number.
    assert_eq!(strip_zsh_extended_prefix(": :0;cmd"), ": :0;cmd"); // Empty timestamp.
    assert_eq!(strip_zsh_extended_prefix(": 123:;cmd"), ": 123:;cmd"); // Empty elapsed.
}

#[test]
fn test_parse_history_zsh() {
    let history_lines = "
cat ~/.zsh_history
a multi-line\\
command
: 1699251735:0;an extended history command
: 1699251735:0;a multi-line extended\\
history command
";
    assert_eq!(
        ShellType::Zsh.parse_history(history_lines.as_bytes()),
        vec![
            "cat ~/.zsh_history".to_string(),
            "a multi-line\ncommand".to_string(),
            "an extended history command".to_string(),
            "a multi-line extended\nhistory command".to_string(),
        ]
    );
}

#[test]
fn test_parse_history_zsh_continuation_line_looks_like_prefix() {
    // Regression test: a continuation line that happens to match the extended history
    // prefix pattern should NOT be stripped.
    let history_lines = ": 1699251735:0;echo '\\
: 9999:0;fake prefix'";
    assert_eq!(
        ShellType::Zsh.parse_history(history_lines.as_bytes()),
        vec!["echo '\n: 9999:0;fake prefix'".to_string()]
    );
}

#[test]
fn test_zsh_unmetafy() {
    let test_zsh_history = [
        227, 129, 131, 179, 227, 130, 131, 172, 227, 129, 175, 230, 131, 183, 165, 230, 131, 188,
        172, 232, 170, 131, 190, 227, 129, 167, 227, 129, 131, 185,
    ];
    let unmetafied_test = zsh_unmetafy(&test_zsh_history);
    assert_eq!("これは日本語です", &unmetafied_test);
}

#[test]
fn test_fish_unescape_history_yaml() {
    assert_eq!(fish_unescape_history_yaml("foo"), "foo");
    assert_eq!(fish_unescape_history_yaml("foo\\nbar"), "foo\nbar");
    assert_eq!(fish_unescape_history_yaml("foo\\\\"), "foo\\");
    assert_eq!(fish_unescape_history_yaml("foo\\"), "foo"); // trailing escape dropped
}

#[test]
fn test_from_name() {
    assert_eq!(Some(ShellType::Bash), ShellType::from_name("bash"));
    assert_eq!(Some(ShellType::Bash), ShellType::from_name("-bash"));
    assert_eq!(Some(ShellType::Bash), ShellType::from_name("/bin/bash"));
    assert_eq!(Some(ShellType::Bash), ShellType::from_name("/usr/bin/bash"));
    assert_eq!(Some(ShellType::Zsh), ShellType::from_name("/bin/zsh"));
    assert_eq!(None, ShellType::from_name("/bin/zsh/foo"));
    assert_eq!(None, ShellType::from_name("/bin/zsh/-bash"));
    assert_eq!(None, ShellType::from_name("rezsh"));
    assert_eq!(Some(ShellType::Fish), ShellType::from_name("fish"));
    assert_eq!(
        Some(ShellType::Fish),
        ShellType::from_name("/usr/local/bin/fish")
    );
    assert_eq!(
        Some(ShellType::PowerShell),
        ShellType::from_name("pwsh.exe")
    );
    assert_eq!(None, ShellType::from_name("pwsh.bat"));
    assert_eq!(
        Some(ShellType::PowerShell),
        ShellType::from_name("/usr/bin/env/powershell")
    );
    assert_eq!(
        Some(ShellType::PowerShell),
        ShellType::from_name("C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe")
    );
    assert_eq!(None, ShellType::from_name("psh"));
}

#[test]
fn test_from_markdown_language_spec() {
    // Standard shell languages
    assert_eq!(
        Some(ShellType::Bash),
        ShellType::from_markdown_language_spec("bash")
    );
    assert_eq!(
        Some(ShellType::Bash),
        ShellType::from_markdown_language_spec("shell")
    );
    assert_eq!(
        Some(ShellType::Bash),
        ShellType::from_markdown_language_spec("sh")
    );
    assert_eq!(
        Some(ShellType::Zsh),
        ShellType::from_markdown_language_spec("zsh")
    );
    assert_eq!(
        Some(ShellType::Fish),
        ShellType::from_markdown_language_spec("fish")
    );
    assert_eq!(
        Some(ShellType::PowerShell),
        ShellType::from_markdown_language_spec("powershell")
    );
    assert_eq!(
        Some(ShellType::PowerShell),
        ShellType::from_markdown_language_spec("pwsh")
    );

    // Non-shell languages and invalid inputs
    assert_eq!(None, ShellType::from_markdown_language_spec("python"));
    assert_eq!(None, ShellType::from_markdown_language_spec("rust"));
    assert_eq!(None, ShellType::from_markdown_language_spec(""));
    // Paths and executable names should not match (use from_name for those)
    assert_eq!(None, ShellType::from_markdown_language_spec("/bin/bash"));
    assert_eq!(None, ShellType::from_markdown_language_spec("-bash"));
}

#[test]
fn test_fish_parse_abbrs() {
    let raw_abbrs = "abbr -a -U -- gco 'git checkout'
abbr -a -g -- gq 'git commit'
abbr -a -U -- ehw 'echo \"Hello, world\"'
abbr -a -- ga 'git add' # imported from a universal variable, see `help abbr`";
    let abbrs = ShellType::Fish.abbreviations(raw_abbrs);

    assert_eq!(abbrs.len(), 4);
    assert_eq!(abbrs.get("gco").unwrap(), "git checkout");
    assert_eq!(abbrs.get("gq").unwrap(), "git commit");
    assert_eq!(abbrs.get("ehw").unwrap(), r#"echo "Hello, world""#);
    assert_eq!(abbrs.get("ga").unwrap(), "git add");
}

#[test]
fn test_fish_parse_aliases() {
    let raw_aliases = "alias g git
alias rmi 'rm -i'
alias ehw 'echo \"Hello, world\"'";
    let aliases = ShellType::Fish.aliases(raw_aliases);

    assert_eq!(aliases.len(), 3);
    assert_eq!(aliases.get("g").unwrap(), "git");
    assert_eq!(aliases.get("rmi").unwrap(), "rm -i");
    assert_eq!(aliases.get("ehw").unwrap(), r#"echo "Hello, world""#);
}

#[test]
fn test_should_add_command_to_history() {
    {
        // Test zsh's "histignorespace" option.
        let options = HashSet::from(["histignorespace".to_string()]);
        let shell = Shell::new(
            ShellType::Zsh,
            None,
            Some(options),
            Default::default(),
            None,
        );

        assert!(shell.should_add_command_to_history("asdf"));
        assert!(!shell.should_add_command_to_history(" asdf"));

        let shell = Shell::new(ShellType::Zsh, None, None, Default::default(), None);
        assert!(shell.should_add_command_to_history("asdf"));
        assert!(shell.should_add_command_to_history(" asdf"));
    }

    // Test our "!histcontrol_" faked option for bash.
    {
        for variant in [
            "!histcontrol_ignorespace",
            "!histcontrol_ignoreboth",
            "!histcontrol_testing:ignorespace",
            "!histcontrol_testing:ignoreboth:testing",
        ] {
            let options = HashSet::from([variant.to_string()]);
            let bash_shell = Shell::new(
                ShellType::Bash,
                None,
                Some(options.clone()),
                Default::default(),
                None,
            );

            assert!(bash_shell.should_add_command_to_history("asdf"));
            assert!(!bash_shell.should_add_command_to_history(" asdf"));

            // Make sure that option only takes effect when the shell is bash.
            let zsh_shell = Shell::new(
                ShellType::Zsh,
                None,
                Some(options),
                Default::default(),
                None,
            );
            assert!(zsh_shell.should_add_command_to_history(" asdf"));

            let bash_shell_no_options =
                Shell::new(ShellType::Bash, None, None, Default::default(), None);
            assert!(bash_shell_no_options.should_add_command_to_history("asdf"));
            assert!(bash_shell_no_options.should_add_command_to_history(" asdf"));
        }

        // Ensure we're not only looking for "!histcontrol".
        {
            let options = HashSet::from(["!histcontrol_testing".to_string()]);
            let bash_shell = Shell::new(
                ShellType::Bash,
                None,
                Some(options),
                Default::default(),
                None,
            );
            assert!(bash_shell.should_add_command_to_history(" asdf"));
        }
    }

    // Fish has no shell options that prevent a command from being written to history.
    {
        let fish_shell = Shell::new(ShellType::Fish, None, None, Default::default(), None);
        assert!(fish_shell.should_add_command_to_history("asdf"));
        assert!(fish_shell.should_add_command_to_history(" asdf"));
    }
}

#[test]
fn powershell_script_block_preserving_status_shape() {
    assert_eq!(
        powershell_script_block_preserving_status(PowerShellBlockInvocation::Call, "git log"),
        "& { [CmdletBinding()] param() git log; if (-not $?) { & { $ErrorActionPreference = 'Ignore'; $PSCmdlet.WriteError([Management.Automation.ErrorRecord]::new([Exception]::new('exit status'), 'ExitStatus', 0, $null)) } } }"
    );
    // PowerShell echoes continuation lines into the block output, so the wrapper must not add any.
    let wrapped =
        powershell_script_block_preserving_status(PowerShellBlockInvocation::DotSource, "a\nb");
    assert!(wrapped.starts_with(". { [CmdletBinding()] param() a\nb; "));
    assert_eq!(wrapped.matches('\n').count(), 1);
}

/// Runs `script` in PowerShell and returns its stdout, or `None` if no PowerShell is installed.
fn run_powershell_script(script: &str) -> Option<String> {
    let path = std::env::temp_dir().join(format!(
        "warp_terminal_pwsh_status_{}_{:?}.ps1",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::write(&path, script).expect("can write temp script");
    let candidates: &[&str] = if cfg!(windows) {
        &["pwsh", "powershell"]
    } else {
        &["pwsh"]
    };
    let output = candidates.iter().find_map(|shell| {
        std::process::Command::new(shell)
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
            ])
            .arg(&path)
            .output()
            .ok()
    });
    let _ = std::fs::remove_file(&path);
    let output = output?;
    assert!(
        output.status.success(),
        "PowerShell failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Some(String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n"))
}

/// Regression test: the agent wraps PowerShell commands in script blocks (to unset pagers, and to
/// keep multi-line commands in one block). Plain script blocks reset `$?` to `$true`, which made the
/// precmd hook report exit code 0 for failing native commands.
#[test]
fn powershell_script_block_preserving_status_reports_native_failures() {
    use PowerShellBlockInvocation::{Call, DotSource};

    // A native command that exits with code 3 on every platform: this PowerShell itself.
    let fail = "& ([System.Diagnostics.Process]::GetCurrentProcess().MainModule.FileName) -NoProfile -NonInteractive -Command 'exit 3'";
    let succeed = "& ([System.Diagnostics.Process]::GetCurrentProcess().MainModule.FileName) -NoProfile -NonInteractive -Command 'exit 0'";
    let cases = [
        // Sanity check of the underlying PowerShell behavior this guards against.
        (format!("& {{ {fail} }}"), "True 3 0 Continue"),
        (
            powershell_script_block_preserving_status(Call, fail),
            "False 3 0 Continue",
        ),
        (
            powershell_script_block_preserving_status(DotSource, fail),
            "False 3 0 Continue",
        ),
        (
            powershell_script_block_preserving_status(Call, &format!("{fail};")),
            "False 3 0 Continue",
        ),
        (
            powershell_script_block_preserving_status(Call, &format!("{succeed}\n{fail}")),
            "False 3 0 Continue",
        ),
        (
            powershell_script_block_preserving_status(Call, &format!("{fail}\n{succeed}")),
            "True 0 0 Continue",
        ),
        // A failing cmdlet records its own error and leaves $LASTEXITCODE untouched.
        (
            powershell_script_block_preserving_status(
                Call,
                "Get-Item -LiteralPath ./does-not-exist 2>$null",
            ),
            "False 0 1 Continue",
        ),
        // How a multi-line agent command with the pager wrapper ends up being run.
        (
            powershell_script_block_preserving_status(
                DotSource,
                &powershell_script_block_preserving_status(Call, &format!("$x = 1\n{fail}")),
            ),
            "False 3 0 Continue",
        ),
    ];

    let mut script = String::new();
    for (wrapped, _) in &cases {
        script.push_str(&format!(
            "$global:LASTEXITCODE = 0; $Error.Clear()\n{wrapped}\n\"$? $global:LASTEXITCODE $($Error.Count) $ErrorActionPreference\"\n"
        ));
    }
    let Some(stdout) = run_powershell_script(&script) else {
        eprintln!("PowerShell is not installed; skipping");
        return;
    };
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), cases.len(), "unexpected output:\n{stdout}");
    for ((wrapped, expected), line) in cases.iter().zip(lines) {
        assert_eq!(line, *expected, "for:\n{wrapped}");
    }
}

#[test]
fn test_powershell_command_with_env_preserves_status_and_restores_env() {
    // A native command that exits with `code` on every platform: this PowerShell itself.
    let exit_with = |code: u8| {
        format!("& ([System.Diagnostics.Process]::GetCurrentProcess().MainModule.FileName) -NoProfile -NonInteractive -Command 'exit {code}'")
    };
    let vars = [
        ("PAGER", "cat"),
        ("GIT_PAGER", "cat"),
        ("FTL_QUOTE_TEST", "it's"),
    ];
    let wrap = |command: &str| {
        powershell_command_with_env(
            &vars,
            &format!("\"inner=$env:PAGER|$env:GIT_PAGER|$env:FTL_QUOTE_TEST\"; {command}"),
        )
    };
    let print_env =
        r#""outer=$env:PAGER|$($null -eq $env:GIT_PAGER)|$($null -eq $env:FTL_QUOTE_TEST)""#;
    let script = [
        "$env:PAGER = 'less'".to_owned(),
        "Remove-Item Env:GIT_PAGER, Env:FTL_QUOTE_TEST -ErrorAction Ignore".to_owned(),
        wrap(&exit_with(3)),
        "\"status=$? code=$LASTEXITCODE\"".to_owned(),
        print_env.to_owned(),
        wrap(&exit_with(0)),
        "\"status=$? code=$LASTEXITCODE\"".to_owned(),
        print_env.to_owned(),
        // The environment is restored even if the command throws.
        format!(
            "try {{ {} }} catch {{ \"caught=$_\" }}",
            wrap("throw 'boom'")
        ),
        print_env.to_owned(),
    ]
    .join("\n");

    let Some(stdout) = run_powershell_script(&script) else {
        eprintln!("PowerShell is not installed; skipping");
        return;
    };
    assert_eq!(
        stdout.lines().collect::<Vec<_>>(),
        [
            "inner=cat|cat|it's",
            "status=False code=3",
            "outer=less|True|True",
            "inner=cat|cat|it's",
            "status=True code=0",
            "outer=less|True|True",
            "inner=cat|cat|it's",
            "caught=boom",
            "outer=less|True|True",
        ]
    );
}
