use std::collections::HashMap;
use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::util::mkdtemp;

fn data_dir() -> Option<PathBuf> {
    if let Some(val) = env::var_os("XDG_DATA_HOME") {
        Some(PathBuf::from(val))
    } else if let Some(val) = env::var_os("HOME") {
        Some(PathBuf::from(val).join(concat!(".local/share/", env!("CARGO_CRATE_NAME"))))
    } else {
        None
    }
}

fn nix_profile_dir() -> Option<PathBuf> {
    let val = env::var_os("HOME")?;
    let path = PathBuf::from(val).join(".nix-profile");
    match path.metadata() {
        Ok(_) => Some(path),
        Err(_) => None,
    }
}

pub struct Config {
    pub chroot_dir: PathBuf,
    pub nix_profile: Option<PathBuf>,

    pub env: HashMap<&'static str, OsString>,
    pub nix_home: PathBuf,
}

impl Config {
    pub fn new(use_nix_profile: bool) -> Option<Self> {
        let data_dir = data_dir()?;

        let chroot_dir = mkdtemp(concat!(env!("CARGO_CRATE_NAME"), "-chroot.XXXXXX"))
            .unwrap_or_else(|err| panic!("failed to create temporary directory: {}", err));

        let nix_profile = if use_nix_profile {
            nix_profile_dir()
        } else {
            None
        };

        let env: HashMap<&'static str, OsString> = HashMap::from([
            ("SHELL", "/bin/sh".into()),
            ("NIXBOX_BINDIR", data_dir.join("bin").into()),
            ("XDG_DATA_HOME", data_dir.join("data").into()),
            ("XDG_STATE_HOME", data_dir.join("state").into()),
            ("XDG_CONFIG_HOME", data_dir.join("config").into()),
            ("NIX_CONF_DIR", "/nix/etc/nix".into()),
            (
                "NIXBOX_EXECUTABLE",
                env::current_exe()
                    .unwrap_or_else(|err| panic!("current_exe() could not be called: {}", err))
                    .into(),
            ),

            (
            "PATH",
            match nix_profile.clone() {
                Some(x) => {
                    let mut os: OsString = x.into_os_string();
                    os.push(":/usr/bin:/bin");
                    os
                }
                None => OsString::from("/usr/local/bin:/usr/bin:/bin"),
            },
        )]);

        Some(Self {
            chroot_dir,
            nix_profile,
            env,
            nix_home: data_dir.join("nix"),
        })
    }

    pub fn xdg_data_home(&self) -> &Path {
        &Path::new(
            self.env
                .get("XDG_DATA_HOME")
                .expect("Logic error: env HashMap does not contain XDG_DATA_HOME"),
        )
    }

    pub fn xdg_state_home(&self) -> &Path {
        &Path::new(
            self.env
                .get("XDG_STATE_HOME")
                .expect("Logic error: env HashMap does not contain XDG_STATE_HOME"),
        )
    }

    pub fn nixbox_bindir(&self) -> &Path {
        &Path::new(
            self.env
                .get("NIXBOX_BINDIR")
                .expect("Logic error: env HashMap does not contain NIXBOX_BINDIR"),
        )
    }

    pub fn xdg_config_home(&self) -> &Path {
        &Path::new(
            self.env
                .get("XDG_CONFIG_HOME")
                .expect("Logic error: env HashMap does not contain XDG_CONFIG_HOME"),
        )
    }
}
