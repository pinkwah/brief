mod app;
mod bind;
mod command;
mod config;
mod init;
mod setup;
mod status;
mod table;
mod util;

use std::env;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::{exit, ExitCode};
use std::thread::sleep;
use std::time::Duration;

use clap::{Parser, Subcommand};
use nix::fcntl::{open, OFlag};
use nix::sched::{setns, CloneFlags};
use nix::sys::stat::Mode;
use nix::unistd::{chroot, fork, ForkResult};

use crate::command::install;
use crate::init::Service;
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

    App {
        #[command(subcommand)]
        command: AppCommand,
    },

    Status,
    Init,
    Enter,
    Install,
}

#[derive(Debug, Subcommand)]
enum AppCommand {
    List,
    Install,
}

impl AppCommand {
    fn enter(&self) -> ExitCode {
        let config = Config::new(true).unwrap();
        use AppCommand::*;
        match self {
            List => app::list(&config),

            Install => {
                eprintln!("Not implemented");
                ExitCode::FAILURE
            }
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    use Command::*;
    match cli.command {
        Run {
            no_nix_profile: _,
            rest,
        } => {
            let service = get_or_init_service();
            let config = Config::from(&service);
            enterns(&service);

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

            run(&config, &rest[0], &rest[1..], envs)
        }

        App { command } => command.enter(),

        Enter => {
            let service = get_or_init_service();
            let config = Config::new(true).unwrap();
            enterns(&service);

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

        Init => opt2exit(Service::init()),

        Install => {
            let config = Config::new(false).unwrap();
            // cleanup_config(&config);
            install(&config)
        }

        Status => status::status(),
    }
}

fn get_or_init_service() -> Service {
    if let Some(service) = Service::from_existing() {
        return service;
    }

    match unsafe { fork() } {
        Ok(ForkResult::Parent { .. }) => wait_for_service(),
        Ok(ForkResult::Child) => match unsafe { fork() } {
            Ok(ForkResult::Parent { .. }) => exit(0),
            Ok(ForkResult::Child) => {
                Service::init().unwrap();
                exit(0)
            }
            Err(err) => panic!("fork failed: {}", err),
        },
        Err(err) => panic!("fork failed: {}", err),
    }
}

fn enterns(service: &Service) {
    let cwd = env::current_dir().expect("cannot get current working directory");
    let ns = Path::new("/proc").join(service.pid.to_string()).join("ns");
    if !ns.exists() {
        eprintln!("nixbox initial process not started: run 'nixbox init'");
        exit(1);
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
    chroot(&service.root)
        .unwrap_or_else(|err| panic!("chroot({}): {}", service.root.display(), err));
    env::set_current_dir(&cwd).unwrap_or_else(|err| {
        eprintln!("cannot change directory back to {}: {}", cwd.display(), err)
    });
}

fn wait_for_service() -> Service {
    const ATTEMPTS: i32 = 10;
    for _ in 0..ATTEMPTS {
        if let Some(service) = Service::from_existing() {
            return service;
        }
        sleep(Duration::from_millis(100));
    }
    eprintln!("nixbox initial process not started");
    exit(1);
}

fn opt2exit<T>(var: Option<T>) -> ExitCode {
    var.map(|_| ExitCode::SUCCESS).unwrap_or(ExitCode::FAILURE)
}
