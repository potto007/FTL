use super::*;

#[test]
fn shell_quote_arg_preserves_backslashes_before_quotes() {
    let value = "path\\'; literal\\";
    for (shell, expected) in [
        (ShellType::Bash, "'path\\'\"'\"'; literal\\'"),
        (ShellType::Zsh, "'path\\'\"'\"'; literal\\'"),
        (ShellType::PowerShell, "'path\\''; literal\\'"),
        (ShellType::Fish, "'path\\\\\\'; literal\\\\'"),
    ] {
        assert_eq!(shell_quote_arg(value, shell), expected, "{shell:?}");
        assert_eq!(shell_quote_arg("", shell), "''");
    }
}
