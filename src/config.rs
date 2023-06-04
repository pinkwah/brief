use std::collections::HashMap;
use std::env;
use std::ffi::OsString;
use std::io::Result;
use std::path::{Path, PathBuf};

use crate::util::{mkdtemp, resolve_symlink};

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

pub struct Config {
    pub chroot_dir: PathBuf,
    pub nix_profile: Option<PathBuf>,
    pub current_system: Option<PathBuf>,

    pub env: HashMap<&'static str, OsString>,
    pub nix_home: PathBuf,
}

impl Config {
    pub fn new(use_nix_profile: bool) -> Option<Self> {
        let data_dir = data_dir()?;

        let chroot_dir = mkdtemp(concat!(env!("CARGO_CRATE_NAME"), "-chroot.XXXXXX"))
            .unwrap_or_else(|err| panic!("failed to create temporary directory: {}", err));

        let (nix_profile, current_system) = if use_nix_profile {
            (nix_profile_dir(), current_system_dir())
        } else {
            (None, None)
        };

        let env: HashMap<&'static str, OsString> = HashMap::from([
            ("SHELL", "/bin/sh".into()),
            ("NIXBOX_BINDIR", data_dir.join("bin").into()),
            ("NIXBOX_ROOT", data_dir.join("root").into()),
            (
                "NIXOS_CONFIG",
                data_dir.join("nixbox-configuration.nix").into(),
            ),
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
            ),
        ]);

        Some(Self {
            chroot_dir,
            nix_profile,
            current_system,
            env,
            nix_home: data_dir.join("nix"),
        })
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
}
