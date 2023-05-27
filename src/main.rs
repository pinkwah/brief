mod bind;
mod command;
mod config;
mod setup;
mod util;

use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::PathBuf;
use std::process::{exit, ExitCode};

use clap::{Parser, Subcommand};
use nix::sys::signal::{kill, Signal};
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::{fork, getpid, ForkResult};

use crate::command::install;
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

    Enter,

    Install,
}

fn cleanup(f: impl FnOnce() -> ()) {
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
        Ok(ForkResult::Child) => return,
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

            let envs: Vec<(String, String)> = vec![];
            run(&config, &rest[0], &rest[1..], envs)
        }

        Enter => {
            let config = Config::new(true).unwrap();
            cleanup_config(&config);
            setup(&config);

            let mut shell = PathBuf::from(
                env::var_os("NIXBOX_SHELL")
                    .unwrap_or(OsString::from("/run/current-system/sw/bin/bash")),
            );
            if shell.is_relative() {
                shell = PathBuf::from(env::var_os("HOME").unwrap())
                    .join(".nix-profile/bin")
                    .join(shell);
            }

            let envs = vec![("SHELL", &shell)];
            run(&config, "bash", &["-lc", AsRef::<OsStr>::as_ref(&shell).to_str().unwrap()], envs)
        }

        Install => {
            let config = Config::new(false).unwrap();
            cleanup_config(&config);
            install(&config)
        }
    }
}
