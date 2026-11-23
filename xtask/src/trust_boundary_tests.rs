use super::*;
use std::{env, fs, process::Command};

#[test]
fn passes_two_pr_sequence_both_scoped_and_unscoped_while_rejecting_misuse() {
    let fake_trailer = CommitTouch {
        sha: "bad0".to_string(),
        message: "feat: impl pretending to be verifier\n\nTrust-Boundary: verifier\n".to_string(),
        files: vec!["crates/quota-core/src/lib.rs".to_string()],
    };
    assert!(evaluate_commit_range(&[fake_trailer]).is_err());

    let mixed_with_trailer = CommitTouch {
        sha: "bad1".to_string(),
        message: "feat: coupled\n\nTrust-Boundary: verifier\n".to_string(),
        files: vec![
            "crates/quota-core/src/lib.rs".to_string(),
            "crates/quota-core/tests/t.rs".to_string(),
        ],
    };
    assert!(evaluate_commit_range(&[mixed_with_trailer]).is_err());

    let tmp = env::temp_dir().join(format!("tb-2pr-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(tmp.join("crates/quota-core/src")).unwrap();
    fs::create_dir_all(tmp.join("crates/quota-core/tests")).unwrap();
    let git = |args: &[&str]| {
        assert!(
            Command::new("git")
                .current_dir(&tmp)
                .args(args)
                .status()
                .unwrap()
                .success()
        );
    };
    git(&["init", "-b", "main"]);
    git(&["config", "user.name", "Test"]);
    git(&["config", "user.email", "test@example.invalid"]);
    fs::write(tmp.join("crates/quota-core/src/lib.rs"), "// v0\n").unwrap();
    fs::write(tmp.join("crates/quota-core/tests/t.rs"), "// t0\n").unwrap();
    git(&["add", "."]);
    git(&["commit", "-m", "feat: root"]);

    // PR 1: impl-only bugfix/doc merged onto main
    git(&["checkout", "-b", "pr1"]);
    fs::write(tmp.join("crates/quota-core/src/lib.rs"), "// v1\n").unwrap();
    git(&["commit", "-am", "fix: impl doc"]);
    assert!(check_trust_boundary(&tmp, Some("main")).is_ok());
    git(&["checkout", "main"]);
    git(&["merge", "--no-ff", "-m", "Merge PR 1", "pr1"]);
    assert!(check_trust_boundary(&tmp, None).is_ok());

    // PR 2: trailered verifier commit + impl commit
    git(&["checkout", "-b", "pr2"]);
    fs::write(tmp.join("crates/quota-core/tests/t.rs"), "// t2\n").unwrap();
    git(&[
        "commit",
        "-am",
        "test: contract\n\nTrust-Boundary: verifier",
    ]);
    fs::write(tmp.join("crates/quota-core/src/lib.rs"), "// v2\n").unwrap();
    git(&["commit", "-am", "feat: satisfy contract"]);
    assert!(check_trust_boundary(&tmp, Some("main")).is_ok());
    assert!(check_trust_boundary(&tmp, None).is_ok());

    git(&["checkout", "main"]);
    git(&["merge", "--no-ff", "-m", "Merge PR 2", "pr2"]);
    assert!(check_trust_boundary(&tmp, None).is_ok());
    let _ = fs::remove_dir_all(&tmp);
}
