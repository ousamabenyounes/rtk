use anyhow::{Context, Result};
use serde_json::Value;
use std::io::{self, Read};

/// Run the Gemini CLI BeforeTool hook.
/// Reads JSON from stdin, rewrites shell commands to rtk equivalents,
/// outputs JSON to stdout in Gemini CLI format.
pub fn run_gemini() -> Result<()> {
    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .context("Failed to read hook input from stdin")?;

    let json: Value = serde_json::from_str(&input).context("Failed to parse hook input as JSON")?;

    let tool_name = json.get("tool_name").and_then(|v| v.as_str()).unwrap_or("");

    if tool_name != "run_shell_command" {
        print_allow();
        return Ok(());
    }

    let cmd = json
        .pointer("/tool_input/command")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if cmd.is_empty() {
        print_allow();
        return Ok(());
    }

    // Skip if already using rtk
    if cmd.starts_with("rtk ") || cmd.contains("/rtk ") {
        print_allow();
        return Ok(());
    }

    // Skip heredocs
    if cmd.contains("<<") {
        print_allow();
        return Ok(());
    }

    // Strip leading env var assignments for pattern matching
    let (env_prefix, match_cmd) = strip_env_prefix(cmd);

    if let Some(rewritten) = try_rewrite(match_cmd) {
        let full_rewrite = if env_prefix.is_empty() {
            rewritten
        } else {
            format!("{}{}", env_prefix, rewritten)
        };
        print_rewrite(&full_rewrite);
    } else {
        print_allow();
    }

    Ok(())
}

fn print_allow() {
    println!(r#"{{"decision":"allow"}}"#);
}

fn print_rewrite(cmd: &str) {
    let output = serde_json::json!({
        "decision": "allow",
        "hookSpecificOutput": {
            "tool_input": {
                "command": cmd
            }
        }
    });
    println!("{}", output);
}

/// Strip leading env var assignments (e.g., "FOO=bar BAZ=1 git status" -> ("FOO=bar BAZ=1 ", "git status"))
fn strip_env_prefix(cmd: &str) -> (&str, &str) {
    let bytes = cmd.as_bytes();
    let mut i = 0;
    let len = bytes.len();

    loop {
        // Try to match: [A-Za-z_][A-Za-z0-9_]*=[^ ]* +
        let start = i;

        // First char must be letter or underscore
        if i >= len || !(bytes[i].is_ascii_alphabetic() || bytes[i] == b'_') {
            break;
        }
        i += 1;

        // Rest of var name: alphanumeric or underscore
        while i < len && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
            i += 1;
        }

        // Must have '='
        if i >= len || bytes[i] != b'=' {
            // Not an env var assignment, revert
            i = start;
            break;
        }
        i += 1; // skip '='

        // Value: non-space chars
        while i < len && bytes[i] != b' ' {
            i += 1;
        }

        // Must have at least one space after value
        if i >= len || bytes[i] != b' ' {
            i = start;
            break;
        }

        // Skip spaces
        while i < len && bytes[i] == b' ' {
            i += 1;
        }

        // Check if next thing is another env var or a command
        // Peek: if next segment looks like VAR=val, continue; else stop
        let peek = i;
        let mut j = peek;
        if j < len && (bytes[j].is_ascii_alphabetic() || bytes[j] == b'_') {
            j += 1;
            while j < len && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                j += 1;
            }
            if j < len && bytes[j] == b'=' {
                // Looks like another env var, continue the loop
                continue;
            }
        }
        // Next segment is the command, stop here
        break;
    }

    if i == 0 {
        ("", cmd)
    } else {
        (&cmd[..i], &cmd[i..])
    }
}

/// Try to rewrite a command to its rtk equivalent. Returns None if no rewrite.
fn try_rewrite(cmd: &str) -> Option<String> {
    let first_word = cmd.split_whitespace().next().unwrap_or("");
    let rest = cmd.get(first_word.len()..).unwrap_or("").trim_start();

    match first_word {
        // --- Git ---
        "git" => try_rewrite_git(cmd, rest),

        // --- GitHub CLI ---
        "gh" => try_rewrite_gh(rest, cmd),

        // --- Cargo ---
        "cargo" => try_rewrite_cargo(cmd, rest),

        // --- File operations ---
        "cat" => Some(format!("rtk read {}", rest)),
        "rg" | "grep" => Some(format!("rtk grep {}", rest)),
        "ls" => Some(format!("rtk ls{}", &cmd[2..])), // preserve "ls -la" spacing
        "tree" => Some(format!("rtk tree{}", &cmd[4..])),
        "find" => Some(format!("rtk find {}", rest)),
        "diff" => Some(format!("rtk diff {}", rest)),
        "head" => try_rewrite_head(rest),

        // --- JS/TS tooling ---
        "vitest" => try_rewrite_vitest(cmd),
        "npx" => try_rewrite_npx(rest),
        "pnpm" => try_rewrite_pnpm(rest),
        "npm" => try_rewrite_npm(rest),
        "vue-tsc" => Some(format!(
            "rtk tsc{}",
            if rest.is_empty() {
                String::new()
            } else {
                format!(" {}", rest)
            }
        )),
        "tsc" => Some(format!(
            "rtk tsc{}",
            if rest.is_empty() {
                String::new()
            } else {
                format!(" {}", rest)
            }
        )),
        "eslint" => Some(format!(
            "rtk lint{}",
            if rest.is_empty() {
                String::new()
            } else {
                format!(" {}", rest)
            }
        )),
        "prettier" => Some(format!(
            "rtk prettier{}",
            if rest.is_empty() {
                String::new()
            } else {
                format!(" {}", rest)
            }
        )),
        "playwright" => Some(format!(
            "rtk playwright{}",
            if rest.is_empty() {
                String::new()
            } else {
                format!(" {}", rest)
            }
        )),
        "prisma" => Some(format!(
            "rtk prisma{}",
            if rest.is_empty() {
                String::new()
            } else {
                format!(" {}", rest)
            }
        )),

        // --- Containers ---
        "docker" => try_rewrite_docker(rest, cmd),
        "kubectl" => try_rewrite_kubectl(rest, cmd),

        // --- Network ---
        "curl" => Some(format!("rtk curl {}", rest)),
        "wget" => Some(format!("rtk wget {}", rest)),

        // --- Python tooling ---
        "pytest" => Some(format!(
            "rtk pytest{}",
            if rest.is_empty() {
                String::new()
            } else {
                format!(" {}", rest)
            }
        )),
        "python" => try_rewrite_python(rest),
        "ruff" => try_rewrite_ruff(rest),
        "pip" => try_rewrite_pip(rest, cmd),
        "uv" => try_rewrite_uv(rest),

        // --- Go tooling ---
        "go" => try_rewrite_go(rest, cmd),
        "golangci-lint" => Some(format!(
            "rtk golangci-lint{}",
            if rest.is_empty() {
                String::new()
            } else {
                format!(" {}", rest)
            }
        )),

        _ => None,
    }
}

fn try_rewrite_git(cmd: &str, rest: &str) -> Option<String> {
    // Strip git flags like -C, -c, --no-pager to find the actual subcommand
    let subcmd = strip_git_flags(rest);
    let first = subcmd.split_whitespace().next().unwrap_or("");

    match first {
        "status" | "diff" | "log" | "add" | "commit" | "push" | "pull" | "branch" | "fetch"
        | "stash" | "show" => Some(format!("rtk {}", cmd)),
        _ => None,
    }
}

fn strip_git_flags(s: &str) -> String {
    let mut result = String::new();
    let mut iter = s.split_whitespace().peekable();

    while let Some(word) = iter.next() {
        match word {
            "-C" | "-c" => {
                // Skip the next arg (value)
                iter.next();
            }
            w if w.starts_with("--") && w.contains('=') => {
                // --key=value flags, skip
            }
            "--no-pager" | "--no-optional-locks" | "--bare" | "--literal-pathspecs" => {
                // Skip known boolean flags
            }
            _ => {
                if !result.is_empty() {
                    result.push(' ');
                }
                result.push_str(word);
            }
        }
    }
    result
}

fn try_rewrite_gh(rest: &str, _cmd: &str) -> Option<String> {
    let subcmd = rest.split_whitespace().next().unwrap_or("");
    match subcmd {
        "pr" | "issue" | "run" | "api" | "release" => Some(format!("rtk gh {}", rest)),
        _ => None,
    }
}

fn try_rewrite_cargo(cmd: &str, rest: &str) -> Option<String> {
    // Skip toolchain spec like +nightly
    let effective = if rest.starts_with('+') {
        rest.split_whitespace()
            .skip(1)
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        rest.to_string()
    };

    let subcmd = effective.split_whitespace().next().unwrap_or("");
    match subcmd {
        "test" | "build" | "clippy" | "check" | "install" | "fmt" | "nextest" => {
            Some(format!("rtk {}", cmd))
        }
        _ => None,
    }
}

fn try_rewrite_head(rest: &str) -> Option<String> {
    let parts: Vec<&str> = rest.split_whitespace().collect();
    if parts.len() >= 2 {
        // head -N file
        if let Some(n) = parts[0].strip_prefix('-') {
            if n.chars().all(|c| c.is_ascii_digit()) {
                let file = parts[1..].join(" ");
                return Some(format!("rtk read {} --max-lines {}", file, n));
            }
        }
        // head --lines=N file
        if let Some(n) = parts[0].strip_prefix("--lines=") {
            if n.chars().all(|c| c.is_ascii_digit()) {
                let file = parts[1..].join(" ");
                return Some(format!("rtk read {} --max-lines {}", file, n));
            }
        }
    }
    None
}

fn try_rewrite_vitest(cmd: &str) -> Option<String> {
    // vitest -> rtk vitest run
    // vitest run -> rtk vitest run
    // vitest run --reporter -> rtk vitest run --reporter
    let rest = cmd.strip_prefix("vitest").unwrap_or("").trim_start();
    if rest.is_empty() {
        Some("rtk vitest run".to_string())
    } else if rest.starts_with("run") {
        Some(format!("rtk vitest {}", rest))
    } else {
        Some(format!("rtk vitest run {}", rest))
    }
}

fn try_rewrite_npx(rest: &str) -> Option<String> {
    let tool = rest.split_whitespace().next().unwrap_or("");
    let tool_rest = rest.get(tool.len()..).unwrap_or("").trim_start();

    match tool {
        "vitest" => {
            if tool_rest.is_empty() {
                Some("rtk vitest run".to_string())
            } else if tool_rest.starts_with("run") {
                Some(format!("rtk vitest {}", tool_rest))
            } else {
                Some(format!("rtk vitest run {}", tool_rest))
            }
        }
        "vue-tsc" | "tsc" => Some(format!(
            "rtk tsc{}",
            if tool_rest.is_empty() {
                String::new()
            } else {
                format!(" {}", tool_rest)
            }
        )),
        "eslint" => Some(format!(
            "rtk lint{}",
            if tool_rest.is_empty() {
                String::new()
            } else {
                format!(" {}", tool_rest)
            }
        )),
        "prettier" => Some(format!(
            "rtk prettier{}",
            if tool_rest.is_empty() {
                String::new()
            } else {
                format!(" {}", tool_rest)
            }
        )),
        "playwright" => Some(format!(
            "rtk playwright{}",
            if tool_rest.is_empty() {
                String::new()
            } else {
                format!(" {}", tool_rest)
            }
        )),
        "prisma" => Some(format!(
            "rtk prisma{}",
            if tool_rest.is_empty() {
                String::new()
            } else {
                format!(" {}", tool_rest)
            }
        )),
        _ => None,
    }
}

fn try_rewrite_pnpm(rest: &str) -> Option<String> {
    let subcmd = rest.split_whitespace().next().unwrap_or("");
    let sub_rest = rest.get(subcmd.len()..).unwrap_or("").trim_start();

    match subcmd {
        "vitest" => {
            if sub_rest.is_empty() {
                Some("rtk vitest run".to_string())
            } else if sub_rest.starts_with("run") {
                Some(format!("rtk vitest {}", sub_rest))
            } else {
                Some(format!("rtk vitest run {}", sub_rest))
            }
        }
        "test" => Some(format!(
            "rtk vitest run{}",
            if sub_rest.is_empty() {
                String::new()
            } else {
                format!(" {}", sub_rest)
            }
        )),
        "tsc" => Some(format!(
            "rtk tsc{}",
            if sub_rest.is_empty() {
                String::new()
            } else {
                format!(" {}", sub_rest)
            }
        )),
        "lint" => Some(format!(
            "rtk lint{}",
            if sub_rest.is_empty() {
                String::new()
            } else {
                format!(" {}", sub_rest)
            }
        )),
        "playwright" => Some(format!(
            "rtk playwright{}",
            if sub_rest.is_empty() {
                String::new()
            } else {
                format!(" {}", sub_rest)
            }
        )),
        "list" | "ls" | "outdated" => Some(format!("rtk pnpm {}", rest)),
        _ => None,
    }
}

fn try_rewrite_npm(rest: &str) -> Option<String> {
    let subcmd = rest.split_whitespace().next().unwrap_or("");
    let sub_rest = rest.get(subcmd.len()..).unwrap_or("").trim_start();

    match subcmd {
        "test" => Some(format!(
            "rtk npm test{}",
            if sub_rest.is_empty() {
                String::new()
            } else {
                format!(" {}", sub_rest)
            }
        )),
        "run" => Some(format!("rtk npm {}", sub_rest)),
        _ => None,
    }
}

fn try_rewrite_docker(rest: &str, cmd: &str) -> Option<String> {
    let subcmd = rest.split_whitespace().next().unwrap_or("");
    match subcmd {
        "compose" => Some(format!("rtk {}", cmd)),
        "ps" | "images" | "logs" | "run" | "build" | "exec" => Some(format!("rtk {}", cmd)),
        _ => None,
    }
}

fn try_rewrite_kubectl(rest: &str, cmd: &str) -> Option<String> {
    // Strip kubectl flags to find actual subcommand
    let subcmd = strip_kubectl_flags(rest);
    let first = subcmd.split_whitespace().next().unwrap_or("");
    match first {
        "get" | "logs" | "describe" | "apply" => Some(format!("rtk {}", cmd)),
        _ => None,
    }
}

fn strip_kubectl_flags(s: &str) -> String {
    let mut result = String::new();
    let mut iter = s.split_whitespace().peekable();

    while let Some(word) = iter.next() {
        match word {
            "--context" | "--kubeconfig" | "--namespace" | "-n" => {
                iter.next(); // skip value
            }
            w if w.starts_with("--") && w.contains('=') => {
                // skip --key=value
            }
            _ => {
                if !result.is_empty() {
                    result.push(' ');
                }
                result.push_str(word);
            }
        }
    }
    result
}

fn try_rewrite_python(rest: &str) -> Option<String> {
    // python -m pytest ... -> rtk pytest ...
    let parts: Vec<&str> = rest.splitn(3, ' ').collect();
    if parts.len() >= 2 && parts[0] == "-m" && parts[1] == "pytest" {
        let pytest_rest = if parts.len() > 2 { parts[2] } else { "" };
        Some(format!(
            "rtk pytest{}",
            if pytest_rest.is_empty() {
                String::new()
            } else {
                format!(" {}", pytest_rest)
            }
        ))
    } else {
        None
    }
}

fn try_rewrite_ruff(rest: &str) -> Option<String> {
    let subcmd = rest.split_whitespace().next().unwrap_or("");
    match subcmd {
        "check" | "format" => Some(format!("rtk ruff {}", rest)),
        _ => None,
    }
}

fn try_rewrite_pip(rest: &str, _cmd: &str) -> Option<String> {
    let subcmd = rest.split_whitespace().next().unwrap_or("");
    match subcmd {
        "list" | "outdated" | "install" | "show" => Some(format!("rtk pip {}", rest)),
        _ => None,
    }
}

fn try_rewrite_uv(rest: &str) -> Option<String> {
    // uv pip list -> rtk pip list
    if rest.starts_with("pip ") {
        let pip_rest = &rest[4..];
        let subcmd = pip_rest.split_whitespace().next().unwrap_or("");
        match subcmd {
            "list" | "outdated" | "install" | "show" => Some(format!("rtk pip {}", pip_rest)),
            _ => None,
        }
    } else {
        None
    }
}

fn try_rewrite_go(rest: &str, cmd: &str) -> Option<String> {
    let subcmd = rest.split_whitespace().next().unwrap_or("");
    match subcmd {
        "test" | "build" | "vet" => Some(format!("rtk {}", cmd)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- strip_env_prefix ---

    #[test]
    fn test_strip_env_prefix_none() {
        let (prefix, cmd) = strip_env_prefix("git status");
        assert_eq!(prefix, "");
        assert_eq!(cmd, "git status");
    }

    #[test]
    fn test_strip_env_prefix_single() {
        let (prefix, cmd) = strip_env_prefix("GIT_PAGER=cat git status");
        assert_eq!(prefix, "GIT_PAGER=cat ");
        assert_eq!(cmd, "git status");
    }

    #[test]
    fn test_strip_env_prefix_multi() {
        let (prefix, cmd) = strip_env_prefix("NODE_ENV=test CI=1 npx vitest run");
        assert_eq!(prefix, "NODE_ENV=test CI=1 ");
        assert_eq!(cmd, "npx vitest run");
    }

    // --- try_rewrite ---

    #[test]
    fn test_git_status() {
        assert_eq!(try_rewrite("git status"), Some("rtk git status".into()));
    }

    #[test]
    fn test_git_log_flags() {
        assert_eq!(
            try_rewrite("git log --oneline -10"),
            Some("rtk git log --oneline -10".into())
        );
    }

    #[test]
    fn test_git_no_pager() {
        assert_eq!(
            try_rewrite("git --no-pager log"),
            Some("rtk git --no-pager log".into())
        );
    }

    #[test]
    fn test_git_diff() {
        assert_eq!(
            try_rewrite("git diff HEAD"),
            Some("rtk git diff HEAD".into())
        );
    }

    #[test]
    fn test_git_unknown_subcmd() {
        assert_eq!(try_rewrite("git rebase main"), None);
    }

    #[test]
    fn test_gh_pr() {
        assert_eq!(try_rewrite("gh pr list"), Some("rtk gh pr list".into()));
    }

    #[test]
    fn test_gh_unknown() {
        assert_eq!(try_rewrite("gh auth login"), None);
    }

    #[test]
    fn test_cargo_test() {
        assert_eq!(try_rewrite("cargo test"), Some("rtk cargo test".into()));
    }

    #[test]
    fn test_cargo_clippy_flags() {
        assert_eq!(
            try_rewrite("cargo clippy --all-targets"),
            Some("rtk cargo clippy --all-targets".into())
        );
    }

    #[test]
    fn test_cat() {
        assert_eq!(
            try_rewrite("cat package.json"),
            Some("rtk read package.json".into())
        );
    }

    #[test]
    fn test_grep() {
        assert_eq!(
            try_rewrite("grep -rn pattern src/"),
            Some("rtk grep -rn pattern src/".into())
        );
    }

    #[test]
    fn test_rg() {
        assert_eq!(
            try_rewrite("rg pattern src/"),
            Some("rtk grep pattern src/".into())
        );
    }

    #[test]
    fn test_ls() {
        assert_eq!(try_rewrite("ls -la"), Some("rtk ls -la".into()));
    }

    #[test]
    fn test_head_dash_n() {
        assert_eq!(
            try_rewrite("head -20 file.txt"),
            Some("rtk read file.txt --max-lines 20".into())
        );
    }

    #[test]
    fn test_head_lines_eq() {
        assert_eq!(
            try_rewrite("head --lines=10 file.txt"),
            Some("rtk read file.txt --max-lines 10".into())
        );
    }

    #[test]
    fn test_vitest_bare() {
        assert_eq!(try_rewrite("vitest"), Some("rtk vitest run".into()));
    }

    #[test]
    fn test_vitest_run_no_double() {
        assert_eq!(try_rewrite("vitest run"), Some("rtk vitest run".into()));
    }

    #[test]
    fn test_npx_playwright() {
        assert_eq!(
            try_rewrite("npx playwright test"),
            Some("rtk playwright test".into())
        );
    }

    #[test]
    fn test_npx_vitest_run() {
        assert_eq!(try_rewrite("npx vitest run"), Some("rtk vitest run".into()));
    }

    #[test]
    fn test_pnpm_test() {
        assert_eq!(try_rewrite("pnpm test"), Some("rtk vitest run".into()));
    }

    #[test]
    fn test_pnpm_list() {
        assert_eq!(try_rewrite("pnpm list"), Some("rtk pnpm list".into()));
    }

    #[test]
    fn test_npm_test() {
        assert_eq!(try_rewrite("npm test"), Some("rtk npm test".into()));
    }

    #[test]
    fn test_npm_run() {
        assert_eq!(
            try_rewrite("npm run test:e2e"),
            Some("rtk npm test:e2e".into())
        );
    }

    #[test]
    fn test_docker_ps() {
        assert_eq!(try_rewrite("docker ps"), Some("rtk docker ps".into()));
    }

    #[test]
    fn test_docker_compose() {
        assert_eq!(
            try_rewrite("docker compose up -d"),
            Some("rtk docker compose up -d".into())
        );
    }

    #[test]
    fn test_kubectl_get() {
        assert_eq!(
            try_rewrite("kubectl get pods"),
            Some("rtk kubectl get pods".into())
        );
    }

    #[test]
    fn test_curl() {
        assert_eq!(
            try_rewrite("curl -s https://example.com"),
            Some("rtk curl -s https://example.com".into())
        );
    }

    #[test]
    fn test_pytest() {
        assert_eq!(try_rewrite("pytest"), Some("rtk pytest".into()));
    }

    #[test]
    fn test_python_m_pytest() {
        assert_eq!(
            try_rewrite("python -m pytest -v"),
            Some("rtk pytest -v".into())
        );
    }

    #[test]
    fn test_ruff_check() {
        assert_eq!(try_rewrite("ruff check ."), Some("rtk ruff check .".into()));
    }

    #[test]
    fn test_pip_list() {
        assert_eq!(try_rewrite("pip list"), Some("rtk pip list".into()));
    }

    #[test]
    fn test_uv_pip_install() {
        assert_eq!(
            try_rewrite("uv pip install flask"),
            Some("rtk pip install flask".into())
        );
    }

    #[test]
    fn test_go_test() {
        assert_eq!(
            try_rewrite("go test ./..."),
            Some("rtk go test ./...".into())
        );
    }

    #[test]
    fn test_golangci_lint() {
        assert_eq!(
            try_rewrite("golangci-lint run"),
            Some("rtk golangci-lint run".into())
        );
    }

    #[test]
    fn test_echo_no_rewrite() {
        assert_eq!(try_rewrite("echo hello"), None);
    }

    #[test]
    fn test_cd_no_rewrite() {
        assert_eq!(try_rewrite("cd /tmp"), None);
    }

    #[test]
    fn test_node_no_rewrite() {
        assert_eq!(try_rewrite("node -e 'console.log(1)'"), None);
    }

    // --- Full JSON flow ---

    #[test]
    fn test_env_prefix_with_rewrite() {
        let cmd = "TEST_SESSION_ID=2 npx playwright test --config=foo";
        let (prefix, match_cmd) = strip_env_prefix(cmd);
        let rewritten = try_rewrite(match_cmd).unwrap();
        let full = format!("{}{}", prefix, rewritten);
        assert_eq!(full, "TEST_SESSION_ID=2 rtk playwright test --config=foo");
    }

    #[test]
    fn test_vue_tsc() {
        assert_eq!(
            try_rewrite("vue-tsc --noEmit"),
            Some("rtk tsc --noEmit".into())
        );
    }

    #[test]
    fn test_npx_vue_tsc() {
        assert_eq!(
            try_rewrite("npx vue-tsc --noEmit"),
            Some("rtk tsc --noEmit".into())
        );
    }

    #[test]
    fn test_pnpm_tsc() {
        assert_eq!(try_rewrite("pnpm tsc"), Some("rtk tsc".into()));
    }

    #[test]
    fn test_pnpm_lint() {
        assert_eq!(try_rewrite("pnpm lint"), Some("rtk lint".into()));
    }

    #[test]
    fn test_eslint() {
        assert_eq!(try_rewrite("eslint src/"), Some("rtk lint src/".into()));
    }

    #[test]
    fn test_tree() {
        assert_eq!(try_rewrite("tree src/"), Some("rtk tree src/".into()));
    }

    #[test]
    fn test_find() {
        assert_eq!(
            try_rewrite("find . -name '*.ts'"),
            Some("rtk find . -name '*.ts'".into())
        );
    }

    #[test]
    fn test_wget() {
        assert_eq!(
            try_rewrite("wget https://example.com/file"),
            Some("rtk wget https://example.com/file".into())
        );
    }
}
