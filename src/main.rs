mod bind;
mod exec;
mod mkdtemp;
mod resolve_symlink;

use std::{fs, path::Path, process::exit};
use clap::Parser;
use nix::{unistd::{ForkResult, fork, Pid, getpid}, sys::{wait::{WaitPidFlag, WaitStatus, waitpid}, signal::{Signal, kill}}};
use exec::RunChroot;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg()]
    nixdir: String,

    #[arg(short, long, action)]
    no_nix_profile: bool,

    #[arg()]
    rest: Vec<String>,
}

fn wait_for_child(rootdir: &Path, child_pid: Pid) {
    let mut exit_status = 1;
    loop {
        match waitpid(child_pid, Some(WaitPidFlag::WUNTRACED)) {
            Ok(WaitStatus::Signaled(child, Signal::SIGSTOP, _)) => {
                let _ = kill(getpid(), Signal::SIGSTOP);
                let _ = kill(child, Signal::SIGCONT);
            }
            Ok(WaitStatus::Signaled(_, signal, _)) => {
                kill(getpid(), signal).unwrap_or_else(|err| {
                    panic!("failed to send {} signal to our self: {}", signal, err)
                });
            }
            Ok(WaitStatus::Exited(_, status)) => {
                exit_status = status;
                break;
            }
            Ok(what) => {
                eprintln!("unexpected wait event happend: {:?}", what);
                break;
            }
            Err(e) => {
                eprintln!("waitpid failed: {}", e);
                break;
            }
        };
    }

    fs::remove_dir_all(rootdir)
        .unwrap_or_else(|err| panic!("cannot remove tempdir {}: {}", rootdir.display(), err));

    exit(exit_status);
}

fn child(args: Args, chroot: RunChroot) {
    let (uid, gid) = chroot.unshare();
    if args.no_nix_profile {
        chroot.bind_host();
    } else {
        chroot.bind_nix_profile();
    }
    chroot.bind_defaults();
    chroot.exec(uid, gid, &args.rest[0], &args.rest[1..]);
}

fn main() {
    let args = Args::parse();

    let rootdir = mkdtemp::mkdtemp("nix-chroot.XXXXXX")
        .unwrap_or_else(|err| panic!("failed to create temporary directory: {}", err));

    let nixdir = fs::canonicalize(&args.nixdir)
        .unwrap_or_else(|err| panic!("failed to resolve nix directory {}: {}", args.nixdir, err));

    let chroot = RunChroot::new(&rootdir, &nixdir);

    match unsafe { fork() } {
        Ok(ForkResult::Parent { child, .. }) => wait_for_child(&rootdir, child),
        Ok(ForkResult::Child) => child(args, chroot),
        Err(e) => {
            eprintln!("fork failed: {}", e);
        }
    };
}
