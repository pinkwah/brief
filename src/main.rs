mod bind;
mod command;
mod config;
mod init;
mod setup;
mod util;

use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{exit, ExitCode};

use clap::{Parser, Subcommand};
use nix::fcntl::{open, OFlag};
use nix::sched::{setns, CloneFlags};
use nix::sys::signal::{kill, Signal};
use nix::sys::stat::Mode;
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::{chroot, fork, getpid, ForkResult};

use crate::command::install;
use crate::init::{init, nixbox_chroot, nixbox_env, nixbox_pid};
use crate::setup::setup;
use crate::{command::run, config::Config};

#[derive(Parser, Debug)]
#[command(name = "nixbox")]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Run {
        #[arg(short, long, action)]
        no_nix_profile: bool,

        #[arg()]
        rest: Vec<String>,
    },

    Init,
    Enter,
    Install,
}

fn cleanup(f: impl FnOnce()) {
    match unsafe { fork() } {
        Ok(ForkResult::Parent { child, .. }) => {
            let mut exit_status = 1;
            loop {
                use Signal::*;
                match waitpid(child, Some(WaitPidFlag::WUNTRACED)) {
                    Ok(WaitStatus::Signaled(child, SIGSTOP, _)) => {
                        let _ = kill(getpid(), SIGSTOP);
                        let _ = kill(child, SIGCONT);
                    }
                    Ok(WaitStatus::Signaled(_, signal, _)) => kill(getpid(), signal)
                        .unwrap_or_else(|err| {
                            panic!(
                                "failed to send {} signal to {}: {}",
                                signal,
                                env!("CARGO_CRATE_NAME"),
                                err
                            );
                        }),
                    Ok(WaitStatus::Exited(_, status)) => {
                        exit_status = status;
                        break;
                    }
                    Ok(what) => {
                        eprintln!("unexpected wait event: {:?}", what);
                        break;
                    }
                    Err(err) => {
                        eprintln!("waitpid failed: {}", err);
                        break;
                    }
                }
            }

            f();
            exit(exit_status);
        }
        Ok(ForkResult::Child) => (),
        Err(err) => panic!("fork failed: {}", err),
    }
}

fn cleanup_config(config: &Config) {
    cleanup(|| {
        fs::remove_dir_all(&config.chroot_dir).unwrap_or_else(|err| {
            panic!(
                "cannot remove tempdir {}: {}",
                config.chroot_dir.display(),
                err
            );
        });
    });
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    use Command::*;
    match cli.command {
        Run {
            no_nix_profile,
            rest,
        } => {
            let config = Config::new(!no_nix_profile).unwrap();
            cleanup_config(&config);
            setup(&config);

            run(
                &config,
                &rest[0],
                &rest[1..],
                nixbox_env().unwrap_or_default(),
            )
        }

        Enter => {
            let cwd = env::current_dir().expect("cannot get current working directory");
            let config = Config::new(true).unwrap();
            let Some(nixbox_pid) = nixbox_pid() else {
                eprintln!("nixbox initial process not started: run 'nixbox init'");
                return ExitCode::FAILURE;
            };
            let ns = Path::new("/proc").join(nixbox_pid.to_string()).join("ns");
            if !ns.exists() {
                eprintln!("nixbox initial process not started: run 'nixbox init'");
                return ExitCode::FAILURE;
            }

            for group in ["user", "mnt", "uts"] {
                let entry = ns.join(group);
                let fd = open(&entry, OFlag::O_RDONLY | OFlag::O_CLOEXEC, unsafe {
                    Mode::from_bits_unchecked(0)
                })
                .unwrap();
                setns(fd, unsafe { CloneFlags::from_bits_unchecked(0) })
                    .unwrap_or_else(|err| panic!("Could not setns {}: {}", entry.display(), err));
            }

            env::set_current_dir("/").expect("cannot change directory to /");
            chroot(&nixbox_chroot().unwrap())
                .unwrap_or_else(|err| panic!("chroot({}): {}", config.chroot_dir.display(), err));
            env::set_current_dir(&cwd).unwrap_or_else(|err| {
                eprintln!("cannot change directory back to {}: {}", cwd.display(), err)
            });

            let mut shell = PathBuf::from(
                env::var_os("NIXBOX_SHELL")
                    .unwrap_or(OsString::from("/run/current-system/sw/bin/bash")),
            );
            if shell.is_relative() {
                shell =
                    PathBuf::from(env::var_os("HOME").expect("Environment variable HOME not set"))
                        .join(".nix-profile/bin")
                        .join(shell);
            }

            let envs = vec![("SHELL", &shell)];
            run(
                &config,
                "bash",
                ["-lc", AsRef::<OsStr>::as_ref(&shell).to_str().unwrap()],
                envs,
            )
        }

        Init => init(),

        Install => {
            let config = Config::new(false).unwrap();
            // cleanup_config(&config);
            install(&config)
        }
    }
}
