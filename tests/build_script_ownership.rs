//! A dependency must not inherit permission to change its caller's Git hooks.

use std::env;
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Command;

use async_fs_io::TempDir;
use rhusky::Rhusky;
use serial_test::serial;

struct Environment {
    directory: PathBuf,
    values: [Option<OsString>; 3],
}

impl Environment {
    const NAMES: [&'static str; 3] = ["CI", "CARGO_MANIFEST_DIR", "RHUSKY_REPOSITORY_ROOT"];

    fn capture() -> Self {
        Self {
            directory: env::current_dir().expect("original directory"),
            values: Self::NAMES.map(env::var_os),
        }
    }
}

impl Drop for Environment {
    fn drop(&mut self) {
        for (name, value) in Self::NAMES.into_iter().zip(&self.values) {
            // SAFETY: environment tests are serialized and restore before fixture cleanup.
            unsafe {
                match value {
                    Some(value) => env::set_var(name, value),
                    None => env::remove_var(name),
                }
            }
        }
        env::set_current_dir(&self.directory).expect("restore directory");
    }
}

#[test]
#[serial]
fn dependency_cannot_reconfigure_the_opted_in_parent_checkout() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime");
    let owner = runtime
        .block_on(TempDir::create(env::temp_dir()))
        .expect("owner checkout");
    let dependency = runtime
        .block_on(TempDir::create(env::temp_dir()))
        .expect("dependency source");
    assert!(
        Command::new("git")
            .arg("init")
            .arg(owner.path())
            .output()
            .expect("git init")
            .status
            .success()
    );
    assert!(
        Command::new("git")
            .current_dir(owner.path())
            .args(["config", "core.hooksPath", "owned-hooks"])
            .status()
            .expect("set owner hooks")
            .success()
    );
    let _environment = Environment::capture();
    env::set_current_dir(dependency.path()).expect("dependency working directory");
    // SAFETY: this serialized test restores all variables through Environment.
    unsafe {
        env::remove_var("CI");
        env::set_var("CARGO_MANIFEST_DIR", dependency.path());
        env::set_var("RHUSKY_REPOSITORY_ROOT", owner.path());
    }
    Rhusky::new()
        .hooks_dir("dependency-hooks")
        .with_default_hooks()
        .install_from_build_script(dependency.path())
        .expect("skip non-owning dependency");
    let hooks = Command::new("git")
        .current_dir(owner.path())
        .args(["config", "--get", "core.hooksPath"])
        .output()
        .expect("inspect owner hooks");
    assert_eq!(
        String::from_utf8(hooks.stdout).expect("hook path").trim(),
        "owned-hooks"
    );
    assert!(
        !runtime
            .block_on(async_fs_io::try_exists(
                owner.path().join("dependency-hooks")
            ))
            .expect("hook directory")
    );
}

#[test]
#[serial]
fn explicit_checkout_ignores_an_inherited_parent_opt_in() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let owner = runtime
        .block_on(TempDir::create(env::temp_dir()))
        .expect("owner");
    let dependency = runtime
        .block_on(TempDir::create(env::temp_dir()))
        .expect("explicit checkout");
    for directory in [owner.path(), dependency.path()] {
        assert!(
            Command::new("git")
                .arg("init")
                .arg(directory)
                .output()
                .expect("git init")
                .status
                .success()
        );
    }
    assert!(
        Command::new("git")
            .current_dir(owner.path())
            .args(["config", "core.hooksPath", "owned-hooks"])
            .status()
            .expect("owner hooks")
            .success()
    );
    let _environment = Environment::capture();
    // SAFETY: serialized test restores every variable through Environment.
    unsafe {
        env::remove_var("CI");
        env::set_var("CARGO_MANIFEST_DIR", dependency.path());
        env::set_var("RHUSKY_REPOSITORY_ROOT", owner.path());
    }
    Rhusky::new()
        .hooks_dir("explicit-hooks")
        .install_from_build_script(dependency.path())
        .expect("install in explicit checkout");
    for (directory, expected) in [
        (owner.path(), "owned-hooks"),
        (dependency.path(), "explicit-hooks"),
    ] {
        let result = Command::new("git")
            .current_dir(directory)
            .args(["config", "--get", "core.hooksPath"])
            .output()
            .expect("read hook configuration");
        assert!(result.status.success());
        assert_eq!(
            String::from_utf8(result.stdout).expect("hook path").trim(),
            expected
        );
    }
}
