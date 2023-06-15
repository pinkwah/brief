use std::env;
use std::ffi::OsStr;
use std::os::unix::process::ExitStatusExt;
use std::process::{Command, ExitCode};

use crate::config::Config;

const FORWARD_VARS: &[&str] = &[
    "DBUS_SESSION_BUS_ADDRESS",
    "DESKTOP_SESSION",
    "DISPLAY",
    "GDMSESSION",
    "GDM_LANG",
    "GIO_LAUNCHED_DESKTOP_FILE_PID",
    "GNOME_SETUP_DISPLAY",
    "HOME",
    "INVOCATION_ID",
    "JOURNAL_STREAM",
    "LANG",
    "MANAGERPID",
    "SESSION_MANAGER",
    "SHLVL",
    "SSH_AUTH_SOCK",
    "SYSTEMD_EXEC_PID",
    "TERM",
    "USER",
    "VTE_VERSION",
    "WAYLAND_DISPLAY",
    "XAUTHORITY",
    "XDG_CURRENT_DESKTOP",
    "XDG_RUNTIME_DIR",
    "XDG_SESSION_DESKTOP",
    "XDG_SESSION_TYPE",
    "XMODIFIERS",
];

pub fn run<SP, IA, SA, IE, K, V>(config: &Config, program: SP, args: IA, envs: IE) -> ExitCode
where
    SP: AsRef<OsStr>,
    IA: IntoIterator<Item = SA>,
    SA: AsRef<OsStr>,
    IE: IntoIterator<Item = (K, V)>,
    K: AsRef<OsStr>,
    V: AsRef<OsStr>,
{
    let mut command = Command::new(&program);
    command.args(args).env_clear();

    for key in FORWARD_VARS {
        if let Some(val) = env::var_os(key) {
            command.env(key, val);
        }
    }

    command
        .envs(&config.env)
        .envs(envs)
        .status()
        .map(|x| ExitCode::from(x.into_raw() as u8))
        .unwrap_or_else(|err| {
            eprintln!(
                "failed to execute {}: {}",
                program.as_ref().to_string_lossy(),
                err
            );
            ExitCode::FAILURE
        })
}
