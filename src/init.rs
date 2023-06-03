use std::env;
use std::ffi::{OsStr, OsString};
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Write};
use std::ops::Index;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::{self, ExitCode};
use std::thread::sleep;
use std::time::Duration;

use nix::unistd::{sethostname, unlink};

use crate::config::Config;
use crate::setup::setup;

pub fn init() -> ExitCode {
    let config = Config::new(true).unwrap();

    let rundir = xdg_runtime_dir().join("nixbox");
    if !rundir.is_dir() {
        fs::create_dir(&rundir).expect("Could not create nixbox runtime dir");
    }

    let pidfile = rundir.join("server.pid");
    write_pidfile(pidfile).expect("Could not create pidfile");

    force_symlink(&config.chroot_dir, rundir.join("chroot"))
        .unwrap_or_else(|err| panic!("could not chroot symlink: {}", err));

    setup(&config);
    sethostname("nixbox").unwrap_or_else(|err| eprintln!("Could not set hostname: {}", err));

    println!("nixbox initialised");
    loop {
        sleep(Duration::from_secs(10));
    }
}

pub fn nixbox_pid() -> Option<i32> {
    let file = File::open(xdg_runtime_dir().join("nixbox/server.pid")).ok()?;
    let mut reader = BufReader::new(file);
    let mut line = String::new();
    let _ = reader.read_line(&mut line).ok()?;
    line.trim().parse().ok()
}

pub fn nixbox_chroot() -> Option<PathBuf> {
    fs::read_link(xdg_runtime_dir().join("nixbox/chroot")).ok()
}

pub fn nixbox_env() -> Option<Vec<(OsString, OsString)>> {
    let pid = nixbox_pid()?;
    let file = File::open(format!("/proc/{}/environ", pid)).ok()?;
    let mut reader = BufReader::new(file);
    let mut env = vec![];
    for line in reader.split(b'\0') {
        let line = line.ok()?;
        let index = line.iter().position(|c| *c == b'=')?;
        let (key, val) = line.split_at(index);
        env.push((OsStr::from_bytes(key).into(), OsStr::from_bytes(val).into()));
    }
    Some(env)
}

fn xdg_runtime_dir() -> PathBuf {
    PathBuf::from(&env::var_os("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR is not set"))
}

fn write_pidfile(pidfile: impl AsRef<Path>) -> Option<()> {
    let pidfile = pidfile.as_ref();

    if let Some(pid) = nixbox_pid() {
        if Path::new(&format!("/proc/{}", pid)).exists() {
            return None;
        }
    }
    let mut file = File::create(pidfile).ok()?;
    writeln!(file, "{}", process::id()).ok()?;
    Some(())
}

fn force_symlink(source: impl AsRef<Path>, target: impl AsRef<Path>) -> io::Result<()> {
    let source = source.as_ref();
    let target = target.as_ref();

    if target.is_symlink() {
        unlink(target)?;
    }
    symlink(source, target)
}
