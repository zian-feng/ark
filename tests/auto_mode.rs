//! Exercise the real CLI with isolated stores; never launch an AI provider.
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use chrono::{Duration, Utc};
use serde_json::{Value, json};

struct Fixture {
    directory: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let this = Self {
            directory: tempfile::tempdir().unwrap(),
        };
        fs::create_dir_all(this.home().join(".ark")).unwrap();
        fs::write(this.home().join(".ark/config.toml"), "auto_mode = true\n").unwrap();
        this
    }

    fn home(&self) -> &Path {
        self.directory.path()
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_ark"));
        command
            .env("HOME", self.home())
            .env("CODEX_HOME", self.home().join(".codex"))
            .env("CLAUDE_CONFIG_DIR", self.home().join(".claude"))
            .env("ARK_AUTO_DEBUG", "1")
            .current_dir(self.home());
        command
    }

    fn run(&self, arguments: &[&str]) -> Output {
        let output = self.command().args(arguments).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }

    fn pending(&self, pid: &str) -> PathBuf {
        self.home().join(format!(".ark/pending/{pid}.json"))
    }

    fn pre(&self, provider: &str, pid: &str) {
        self.run(&[
            "_hook",
            "pre",
            "--provider",
            provider,
            "--cwd",
            self.home().to_str().unwrap(),
            "--pid",
            pid,
        ]);
    }

    fn post(&self, pid: &str) -> Output {
        self.run(&["_hook", "post", "--pid", pid])
    }

    fn edit_pending(&self, pid: &str, edit: impl FnOnce(&mut Value)) {
        let path = self.pending(pid);
        let mut entries: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        edit(&mut entries[0]);
        fs::write(path, serde_json::to_vec(&entries).unwrap()).unwrap();
    }

    fn rollout(&self, id: &str, timestamp: chrono::DateTime<Utc>, tail: &str) -> PathBuf {
        let directory = self
            .home()
            .join(".codex/sessions")
            .join(timestamp.format("%Y/%m/%d").to_string());
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join(format!("rollout-test-{id}.jsonl"));
        let metadata = json!({"type": "session_meta", "payload": {
            "id": id, "cwd": self.home(), "timestamp": timestamp, "source": "cli"
        }});
        fs::write(&path, format!("{metadata}\n{tail}")).unwrap();
        path
    }
}

#[test]
fn codex_saves_after_a_long_session_even_with_an_incomplete_tail() {
    let f = Fixture::new();
    f.pre("codex", "100");
    f.edit_pending("100", |entry| {
        entry["started_at"] = json!(Utc::now() - Duration::hours(3))
    });
    f.rollout(
        "long-session",
        Utc::now() - Duration::hours(2),
        "{\"type\":",
    );
    let output = f.post("100");
    assert!(String::from_utf8_lossy(&output.stdout).contains("Ark: saved"));
    assert!(!f.pending("100").exists());
    assert!(String::from_utf8_lossy(&f.run(&["ls"]).stdout).contains("long-session"));
    assert!(f.post("100").stdout.is_empty());
}

#[test]
fn delayed_flush_retries_without_reparsing_unchanged_files() {
    let f = Fixture::new();
    f.pre("codex", "100");
    // A recently modified, older session is not this invocation's session.
    f.rollout("old-session", Utc::now() - Duration::hours(1), "");
    let first = f.post("100");
    assert!(first.stdout.is_empty());
    assert!(String::from_utf8_lossy(&first.stderr).contains("1 changed file(s) parsed"));
    f.edit_pending("100", |entry| entry["next_attempt_at"] = Value::Null);
    let second = f.post("100");
    assert!(String::from_utf8_lossy(&second.stderr).contains("0 changed file(s) parsed"));

    let entries: Value = serde_json::from_slice(&fs::read(f.pending("100")).unwrap()).unwrap();
    let start = chrono::DateTime::parse_from_rfc3339(entries[0]["started_at"].as_str().unwrap())
        .unwrap()
        .with_timezone(&Utc);
    f.rollout("delayed-session", start, "");
    f.edit_pending("100", |entry| entry["next_attempt_at"] = Value::Null);
    let third = f.post("100");
    assert!(String::from_utf8_lossy(&third.stdout).contains("Ark: saved"));
    assert!(String::from_utf8_lossy(&third.stderr).contains("1 changed file(s) parsed"));
    let list = f.run(&["ls"]);
    let list = String::from_utf8_lossy(&list.stdout);
    assert!(list.contains("delayed-session"));
    assert!(!list.contains("old-session"));
}

#[test]
fn does_not_choose_newest_of_multiple_sessions() {
    let f = Fixture::new();
    f.pre("codex", "100");
    f.rollout("session-one", Utc::now(), "");
    f.rollout("session-two", Utc::now(), "");
    let output = f.post("100");
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("multiple codex sessions"));
    assert!(!f.pending("100").exists());
    assert!(!f.home().join(".ark/ark.db").exists());
}

#[test]
fn incomplete_metadata_is_reparsed_only_when_the_file_changes() {
    let f = Fixture::new();
    f.pre("codex", "100");
    let created = Utc::now();
    let path = f.rollout("partial-session", created, "");
    fs::write(&path, "{\"type\":\"session_meta\",\"payload\":").unwrap();
    assert!(f.post("100").stdout.is_empty());
    f.edit_pending("100", |entry| entry["next_attempt_at"] = Value::Null);
    assert!(String::from_utf8_lossy(&f.post("100").stderr).contains("0 changed file(s) parsed"));
    f.rollout("partial-session", created, "");
    f.edit_pending("100", |entry| entry["next_attempt_at"] = Value::Null);
    assert!(String::from_utf8_lossy(&f.post("100").stdout).contains("Ark: saved"));
}

#[test]
fn old_history_is_not_opened_and_retry_backoff_does_not_scan() {
    let f = Fixture::new();
    f.pre("codex", "100");
    let ancient = f.rollout("ancient", Utc::now() - Duration::days(60), "");
    // Invalid but recently modified: the old date directory must not be scanned.
    fs::write(ancient, "invalid JSON\n").unwrap();
    let output = f.post("100");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("0 changed file(s) parsed, 0 error(s)")
    );
    f.edit_pending("100", |entry| {
        entry["next_attempt_at"] = json!(Utc::now() + Duration::minutes(1))
    });
    assert!(f.post("100").stderr.is_empty());
}

#[test]
fn turning_auto_off_clears_pending_without_saving() {
    let f = Fixture::new();
    f.pre("codex", "100");
    f.rollout("ignored", Utc::now(), "");
    fs::write(f.home().join(".ark/config.toml"), "auto_mode = false\n").unwrap();
    assert!(f.post("100").stdout.is_empty());
    assert!(!f.pending("100").exists());
    assert!(!f.home().join(".ark/ark.db").exists());
    f.pre("codex", "100");
    assert!(!f.pending("100").exists());
}

#[test]
fn overlapping_hooked_terminals_do_not_claim_each_others_session() {
    let f = Fixture::new();
    f.pre("codex", "100");
    f.pre("codex", "200");
    f.rollout("other-terminal", Utc::now(), "");
    for pid in ["100", "200"] {
        let output = f.post(pid);
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("overlapping codex commands"));
    }
    assert!(!f.home().join(".ark/ark.db").exists());
}

#[test]
fn claude_uses_project_directory_and_ignores_other_projects_and_subagents() {
    let f = Fixture::new();
    f.pre("claude", "100");
    let encoded: String = f
        .home()
        .to_str()
        .unwrap()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let project = f.home().join(".claude/projects").join(encoded);
    fs::create_dir_all(project.join("parent/subagents")).unwrap();
    fs::write(
        project.join("parent/subagents/child.jsonl"),
        "invalid JSON\n",
    )
    .unwrap();
    let unrelated = f.home().join(".claude/projects/unrelated");
    fs::create_dir_all(&unrelated).unwrap();
    fs::write(unrelated.join("bad.jsonl"), "invalid JSON\n").unwrap();
    let record = json!({"type": "user", "sessionId": "claude-session", "cwd": f.home(), "timestamp": Utc::now(), "message": {"role": "user", "content": "Implement reliable autosave"}});
    fs::write(project.join("claude-session.jsonl"), format!("{record}\n")).unwrap();
    let output = f.post("100");
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .contains("Ark: saved `implement-reliable` (claude)")
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("1 changed file(s) parsed"));
}

#[test]
fn retry_window_excludes_sessions_created_after_command_exit() {
    let f = Fixture::new();
    f.pre("codex", "100");
    f.post("100");
    f.rollout("later-terminal", Utc::now() + Duration::seconds(1), "");
    f.edit_pending("100", |entry| entry["next_attempt_at"] = Value::Null);
    let output = f.post("100");
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("0 candidate(s)"));
    assert!(f.pending("100").exists());
}

#[test]
fn completion_endpoint_lists_saved_ark_keys() {
    let f = Fixture::new();
    f.pre("codex", "100");
    f.rollout("completion-session", Utc::now(), "");
    f.post("100");
    let output = f.run(&["_complete", "aliases"]);
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "codex-session-completi\n"
    );
}

#[cfg(unix)]
#[test]
fn zsh_does_not_spawn_ark_without_pending_and_filters_headless_commands() {
    use std::os::unix::fs::PermissionsExt;
    if Command::new("zsh").arg("--version").output().is_err() {
        return;
    }
    let f = Fixture::new();
    let script = f.run(&["init", "zsh"]);
    let hooks = f.home().join("hooks.zsh");
    fs::write(&hooks, script.stdout).unwrap();
    let bin = f.home().join("bin");
    fs::create_dir_all(&bin).unwrap();
    let mock = bin.join("ark");
    fs::write(
        &mock,
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$HOME/calls\"\n",
    )
    .unwrap();
    fs::set_permissions(&mock, fs::Permissions::from_mode(0o755)).unwrap();
    let output = Command::new("zsh")
        .args([
            "-f",
            "-i",
            "-c",
            r#"
        source "$HOME/hooks.zsh"
        ark_precmd
        ark_preexec 'echo codex'
        ark_preexec 'codex exec "task"'
        ark_preexec 'claude -p "task"'
        [[ ! -f "$HOME/calls" ]] || exit 1
        ark_preexec 'codex .'
        mkdir -p "$HOME/.ark/pending"
        print '[]' > "$HOME/.ark/pending/$$.json"
        ark_precmd
        ark_precmd
    "#,
        ])
        .env("HOME", f.home())
        .env("ZDOTDIR", f.home())
        .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
        .current_dir(f.home())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let calls = fs::read_to_string(f.home().join("calls")).unwrap();
    let lines: Vec<_> = calls.lines().collect();
    assert_eq!(lines.len(), 2, "{calls}");
    assert!(lines[0].starts_with("_hook pre --provider codex --cwd "));
    assert!(lines[1].starts_with("_hook post --pid "));
}
