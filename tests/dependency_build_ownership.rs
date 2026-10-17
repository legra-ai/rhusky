//! A previously locked consumer must not inherit hook writes from our
//! dependencies.

use std::env;

use async_fs_io::{
    TempDir,
    ensure_dir,
    write_bytes,
};
use tokio::process::Command;

#[tokio::test]
async fn upgrading_a_locked_consumer_keeps_its_hooks_unchanged() {
    let directory = TempDir::create(env::temp_dir()).await.expect("consumer");
    let root = directory.path();
    ensure_dir(root.join("src"))
        .await
        .expect("source directory");
    write_bytes(root.join("src/lib.rs"), b"//! Consumer.\n")
        .await
        .expect("source");
    write_bytes(root.join("build.rs"), b"fn main() {}\n")
        .await
        .expect("empty build script");
    write_bytes(
        root.join("Cargo.lock"),
        include_bytes!("fixtures/legacy-consumer.Cargo.lock"),
    )
    .await
    .expect("previous consumer lock");
    let source = env!("CARGO_MANIFEST_DIR").replace('\\', "/");
    write_bytes(root.join("Cargo.toml"), format!(
        "[package]\nname='locked-owner-probe'\nversion='0.1.0'\nedition='2024'\n[build-dependencies]\nrhusky={{path='{source}'}}\n").as_bytes()).await.expect("manifest");
    for args in [
        &["init", "--quiet"][..],
        &["config", "core.hooksPath", "owned-hooks"][..],
    ] {
        assert!(
            Command::new("git")
                .args(args)
                .current_dir(root)
                .output()
                .await
                .expect("Git fixture")
                .status
                .success()
        );
    }
    let result = Command::new(env!("CARGO"))
        .args(["build", "--offline"])
        .env("RHUSKY_REPOSITORY_ROOT", root)
        .env_remove("CI")
        .env_remove("GITHUB_ACTIONS")
        .env("CARGO_TARGET_DIR", root.join("target"))
        .current_dir(root)
        .output()
        .await
        .expect("build real dependency graph");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let result = Command::new("git")
        .args(["config", "--get", "core.hooksPath"])
        .current_dir(root)
        .output()
        .await
        .expect("consumer hooks");
    assert!(result.status.success());
    assert_eq!(
        String::from_utf8(result.stdout).expect("hook path").trim(),
        "owned-hooks"
    );
}
