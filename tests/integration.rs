use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use testdir::testdir;

const TARGET: &str = env!("TARGET");

#[test]
fn run_nix_install() {
    let tempdir: PathBuf = testdir!();

    let result = Command::new("cargo")
        .args(&[
            "run",
            "--target",
            TARGET,
            "install",
            // tempdir.to_str().unwrap(),
            // "--no-nix-profile",
            // "--",
            // "bash",
            // "-c",
            // "curl https://nixos.org/nix/install | bash",
        ])
        .status();
    fs::remove_dir_all(tempdir).unwrap();
    assert!(result.unwrap().success());
}
