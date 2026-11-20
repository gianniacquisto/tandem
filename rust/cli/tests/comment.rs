//! `tandem ticket comment add` through the built binary: the author rule the
//! local door shares with `/mcp`.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

use serde_json::Value;

const BINARY: &str = env!("CARGO_BIN_EXE_tandem");

struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new(name: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("tandem-comment-cli-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("user")).expect("the sandbox is creatable");
        Self { root }
    }

    fn run(&self, agent: Option<&str>, args: &[&str]) -> Value {
        let mut command = Command::new(BINARY);
        command
            .arg("--home")
            .arg(self.root.join("home"))
            .env("HOME", self.root.join("user"))
            .env_remove("TANDEM_AGENT")
            .env_remove("TANDEM_SERVER")
            .env_remove("TANDEM_TOKEN")
            .env_remove("TANDEM_HOME")
            .stdin(Stdio::null());
        if let Some(agent) = agent {
            command.env("TANDEM_AGENT", agent);
        }
        let output: Output = command.args(args).output().expect("the CLI runs");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).expect("the CLI prints JSON")
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn a_comment_without_an_author_is_signed_by_the_named_actor_and_the_human_stays_unnamed() {
    let sandbox = Sandbox::new("author");
    let added = sandbox.run(None, &["ticket", "add", "--title", "discussed"]);
    let id = added["id"].as_str().expect("an id").to_string();

    let author = |agent: Option<&str>, extra: &[&str]| {
        let mut args = vec!["ticket", "comment", "add", "--id", &id, "--body", "a line"];
        args.extend_from_slice(extra);
        sandbox.run(agent, &args)["author"]
            .as_str()
            .expect("an author")
            .to_string()
    };

    assert_eq!(author(Some("builder"), &[]), "builder");
    assert_eq!(author(Some("builder"), &["--author", "  "]), "builder");
    assert_eq!(
        author(Some("builder"), &["--author", "reviewer"]),
        "reviewer"
    );
    assert_eq!(author(None, &[]), "", "the human keeps the unnamed author");
}
