use serde::{Deserialize, Serialize};
use std::env;
use std::ffi::{OsStr, OsString};
use std::fs::{self, File};
use std::io::Result;
use std::io::{BufRead, BufReader, ErrorKind};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use toml;

use crate::util::resolve_symlink;

pub struct Config {
    pub runtime_dir: PathBuf,
    pub nix_profile: Option<PathBuf>,
    pub current_system: Option<PathBuf>,

    pub env: HashMap<OsString, OsString>,
    pub nix_home: PathBuf,
}

#[derive(Serialize, Deserialize, Debug)]
struct ConfigFile {
    shell: Option<String>,
}

fn data_dir() -> Option<PathBuf> {
    if let Some(val) = env::var_os("XDG_DATA_HOME") {
        Some(PathBuf::from(val))
    } else {
        env::var_os("HOME")
            .map(|val| PathBuf::from(val).join(concat!(".local/share/", env!("CARGO_CRATE_NAME"))))
    }
}

fn current_system_dir() -> Option<PathBuf> {
    let datadir = data_dir()?.join("root");
    if datadir.symlink_metadata().is_ok() {
        Some(datadir.join("sw/bin"))
    } else {
        None
    }
}

fn nix_profile_dir() -> Option<PathBuf> {
    let val = env::var_os("HOME")?;
    let path = PathBuf::from(val).join(".nix-profile");
    match path.symlink_metadata() {
        Ok(_) => Some(path),
        Err(_) => None,
    }
}

impl Config {
    pub fn new(use_nix_profile: bool) -> Option<Self> {
        let data_dir = data_dir()?;

        let config_file = Self::read_config_file(data_dir.join("config"));

        let (nix_profile, current_system) = if use_nix_profile {
            (nix_profile_dir(), current_system_dir())
        } else {
            (None, None)
        };

        let self_ = Self {
            runtime_dir: xdg_runtime_dir(),
            nix_profile,
            current_system,
            env: HashMap::new(),
            nix_home: data_dir.join("nix"),
        };

        Some(self_)
    }

    pub fn chroot_dir(&self) -> &Path {
        &self.runtime_dir.join("root")
    }

    pub fn daemon_pid(&self) -> Option<String> {
        let file = File::open(runtime_dir.join("nixbox/server.pid")).ok()?;
        let mut reader = BufReader::new(file);
        let mut line = String::new();
        let _ = reader.read_line(&mut line).ok()?;
        line.trim().to_string()
    }

    pub fn xdg_data_home(&self) -> &Path {
        Path::new(
            self.env
                .get("XDG_DATA_HOME")
                .expect("Logic error: env HashMap does not contain XDG_DATA_HOME"),
        )
    }

    pub fn xdg_state_home(&self) -> &Path {
        Path::new(
            self.env
                .get("XDG_STATE_HOME")
                .expect("Logic error: env HashMap does not contain XDG_STATE_HOME"),
        )
    }

    pub fn nixbox_bindir(&self) -> &Path {
        Path::new(
            self.env
                .get("NIXBOX_BINDIR")
                .expect("Logic error: env HashMap does not contain NIXBOX_BINDIR"),
        )
    }

    pub fn xdg_config_home(&self) -> &Path {
        Path::new(
            self.env
                .get("XDG_CONFIG_HOME")
                .expect("Logic error: env HashMap does not contain XDG_CONFIG_HOME"),
        )
    }

    pub fn nixbox_root(&self) -> &Path {
        Path::new(
            self.env
                .get("NIXBOX_ROOT")
                .expect("Logic error: env HashMap does not contain NIXBOX_ROOT"),
        )
    }

    pub fn resolve_symlink(&self, path: impl AsRef<Path>) -> Result<PathBuf> {
        let mnt = ("/nix", &self.nix_home);
        resolve_symlink(&mnt, path)
    }

    fn insert_env(&mut self, key: impl Into<OsString>, val: impl Into<OsString>) {
        self.env.insert(key.into(), val.into());
    }

    fn insert_envs<I, K, V>(&mut self, vars: I)
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<OsString>,
        V: Into<OsString>,
    {
        for (key, val) in vars.into_iter() {
            self.env.insert(key.into(), var.into())
        }
    }

    fn read_config_file(config_path: impl AsRef<Path>) -> Option<ConfigFile> {
        let config_path = config_path.as_ref();

        let contents = match fs::read_to_string(config_path) {
            Ok(x) => Some(x),
            Err(ref err) if err.kind() == ErrorKind::NotFound => None,
            Err(err) => {
                eprintln!(
                    "Could not read config file at '{}': {}",
                    config_path.display(),
                    err
                );
                None
            }
        }?;

        toml::from_str(&contents)
            .map_err(|err| {
                eprintln!(
                    "Could not parse config file at '{}': {}",
                    config_path.display(),
                    err
                )
            })
            .ok()
    }
}

fn xdg_runtime_dir() -> PathBuf {
    env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .expect("XDG_RUNTIME_DIR is not set")
}

fn env_from_pid(pid: String) -> Vec<(OsString, OsString)> {
    let file = File::open(format!("/proc/{}/environ", pid))
        .unwrap_or_else(|err| panic!("Could not open environment for PID {}: {}", pid, err));
    let reader = BufReader::new(file);
    let mut env = vec![];
    for line in reader.split(b'\0') {
        let line = line.unwrap_or_else(|err| panic!("environ: could not read line: {}", err));
        let Some(index) = line.iter().position(|c| *c == b'=') else { continue; };
        let (key, val) = line.split_at(index);
        env.push((OsStr::from_bytes(key).into(), OsStr::from_bytes(val).into()));
    }
    env
}

fn env_from_scratch(data_dir: &Path) -> Vec<(OsString, OsString)> {
    let mut constants: Vec<(OsString, OsString)> = vec![
        ("SHELL", "/bin/sh"),
        ("NIX_CONF_DIR", "/nix/etc/nix"),
        ("PATH", "/usr/local/bin:/usr/bin:/bin"),
    ]
    .iter()
    .map(|(a, b)| (a.into(), b.into()))
    .collect();

    let mut paths: Vec<(OsString, OsString)> = vec![
        ("NIXOS_CONFIG", data_dir.join("nixbox-configuration.nix")),
        ("NIXBOX_BINDIR", data_dir.join("bin")),
        ("NIXBOX_ROOT", data_dir.join("root")),
        ("XDG_DATA_HOME", data_dir.join("data")),
        ("XDG_STATE_HOME", data_dir.join("state")),
        ("XDG_CONFIG_HOME", data_dir.join("config")),
    ]
    .iter()
    .map(|(a, b)| (a.into(), b.into()))
    .collect();

    constants.append(&mut paths);
    paths
}
