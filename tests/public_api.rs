//! Shipped tests of installation ownership and generated commit validation.

use std::env;

use async_fs_io::{
    TempDir,
    read_string_bounded,
    write_bytes,
};
use rhusky::Rhusky;
use serial_test::serial;
use tokio::process::Command;

#[tokio::test]
#[serial]
async fn generated_hooks_require_pins_and_reject_invalid_headlines() {
    let directory = TempDir::create(env::temp_dir())
        .await
        .expect("temporary repository");
    assert!(
        Command::new("git")
            .arg("init")
            .arg(directory.path())
            .output()
            .await
            .expect("git init")
            .status
            .success()
    );
    let original = env::current_dir().expect("working directory");
    let ci = env::var_os("CI");
    env::set_current_dir(directory.path()).expect("enter test repository");
    // SAFETY: this process's environment tests are serialized; restore before
    // assertions.
    unsafe { env::remove_var("CI") };
    let installed = Rhusky::new().with_default_hooks().install().await;
    // SAFETY: restore the original process state before any assertions.
    unsafe {
        if let Some(value) = ci {
            env::set_var("CI", value);
        }
    }
    env::set_current_dir(original).expect("restore working directory");
    installed.expect("install public async API");
    let hooks = directory.path().join(".githooks");
    let pre_commit = read_string_bounded(hooks.join("pre-commit"), 64 * 1024)
        .await
        .expect("read hook");
    assert!(pre_commit.contains("rustfmt-toolchain"));
    assert!(!pre_commit.contains("+nightly "));
    for (message, valid) in [
        ("fix(test): valid headline\n", true),
        ("oops(deps): invalid\n", false),
        ("fix(test):    \n", false),
        ("fix: missing scope\n", false),
        ("invalid\n\nfix(deps): forged body\n", false),
    ] {
        let file = directory.path().join("message");
        write_bytes(&file, message.as_bytes())
            .await
            .expect("write message");
        let result = Command::new("sh")
            .arg(hooks.join("commit-msg"))
            .arg(&file)
            .output()
            .await
            .expect("validate message");
        assert_eq!(
            result.status.success(),
            valid,
            "headline validation: {message}"
        );
    }
}
