use std::{
    fmt,
    path::{Path, PathBuf}, fs::{self, Permissions}, io::ErrorKind, os::unix::prelude::PermissionsExt,
};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

fn runtime_dir_(project_dirs: &ProjectDirs) -> PathBuf {
    if let Some(path) = project_dirs.runtime_dir() {
        return path.to_path_buf();
    }
    let path = Path::new("/tmp").join(format!("{}-{}", "zohar", env!("CARGO_CRATE_NAME")));
    match fs::create_dir(&path) {
        Ok(()) => {
            return path.to_path_buf();
        },
        Err(err) => {
            if err.kind() == ErrorKind::AlreadyExists {
                fs::set_permissions(&path, Permissions::from_mode(0700)).unwrap();
                return path.to_path_buf();
            } else {
                panic!("Could not create dir: {:?}", err);
            }
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct UserConfig {
    shell: String,
}

impl Default for UserConfig {
    fn default() -> Self {
        Self {
            shell: String::from("bash"),
        }
    }
}

impl fmt::Display for UserConfig {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}",
            ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default()).unwrap()
        )
    }
}

impl UserConfig {
    pub fn from_file_or_default() -> Self {
        let project_dirs = ProjectDirs::from("", "", "nixbox").expect("could not create ProjectDirs");
        let path = project_dirs.config_dir().join("config");
        std::fs::read_to_string(path)
            .ok()
            .and_then(|x| ron::from_str::<Self>(x.as_str()).ok())
            .unwrap_or_else(Self::default)
    }
}

#[derive(Clone)]
pub struct Config {
    pub user_config: UserConfig,
    pub guest_chroot: PathBuf,
    pub guest_home: PathBuf,
    pub guest_nix: PathBuf,
    pub guest_nixos_config: PathBuf,
    pub use_host_root: bool,
}

impl Config {
    pub fn new(user_config: UserConfig) -> Self {
        let dirs = ProjectDirs::from("pink", "Wah", "Nixbox Dev").unwrap();
        let runtime_dir = runtime_dir_(&dirs);

        let guest_chroot = runtime_dir.join("chroot");
        let guest_home = dirs.data_dir().join("home");
        let guest_nix = dirs.data_dir().join("nix");
        let guest_nixos_config = dirs.data_dir().join("nixos-config");

        Self {
            user_config,
            guest_chroot,
            guest_home,
            guest_nix,
            guest_nixos_config,
            use_host_root: true,
        }
    }

    pub fn from_file_or_default() -> Self {
        let user_config = UserConfig::from_file_or_default();
        Self::new(user_config)
    }

    pub fn guest_resolve_symlink(&self, path: impl AsRef<Path>) -> std::io::Result<PathBuf> {
        crate::util::resolve_symlink(&(&Path::new("/nix"), &self.guest_nix), path)
    }
}

// fn data_dir() -> Option<PathBuf> {
//     if let Some(val) = env::var_os("XDG_DATA_HOME") {
//         Some(PathBuf::from(val))
//     } else {
//         env::var_os("HOME")
//             .map(|val| PathBuf::from(val).join(concat!(".local/share/", env!("CARGO_CRATE_NAME"))))
//     }
// }

// fn current_system_dir() -> Option<PathBuf> {
//     let datadir = data_dir()?.join("root");
//     if datadir.symlink_metadata().is_ok() {
//         Some(datadir)
//     } else {
//         None
//     }
// }

// fn nix_profile_dir() -> Option<PathBuf> {
//     let val = env::var_os("HOME")?;
//     let path = PathBuf::from(val).join(".nix-profile");
//     match path.symlink_metadata() {
//         Ok(_) => Some(path),
//         Err(_) => None,
//     }
// }

// impl Config {
//     pub fn new(use_nix_profile: bool) -> Option<Self> {
//         let data_dir = data_dir()?;

//         let chroot_dir = mkdtemp(concat!(env!("CARGO_CRATE_NAME"), "-chroot.XXXXXX"))
//             .unwrap_or_else(|err| panic!("failed to create temporary directory: {}", err));

//         let (nix_profile, current_system) = if use_nix_profile {
//             (nix_profile_dir(), current_system_dir())
//         } else {
//             (None, None)
//         };

//         let env: HashMap<OsString, OsString> = HashMap::from([
//             ("SHELL".into(), "/bin/sh".into()),
//             ("NIXBOX_BINDIR".into(), data_dir.join("bin").into()),
//             ("NIXBOX_ROOT".into(), data_dir.join("root").into()),
//             (
//                 "NIXOS_CONFIG".into(),
//                 data_dir.join("nixbox-configuration.nix").into(),
//             ),
//             ("XDG_DATA_HOME".into(), data_dir.join("data").into()),
//             ("XDG_STATE_HOME".into(), data_dir.join("state").into()),
//             ("XDG_CONFIG_HOME".into(), data_dir.join("config").into()),
//             ("NIX_CONF_DIR".into(), "/nix/etc/nix".into()),
//             (
//                 "NIXBOX_EXECUTABLE".into(),
//                 env::current_exe()
//                     .unwrap_or_else(|err| panic!("current_exe() could not be called: {}", err))
//                     .into(),
//             ),
//             (
//                 "PATH".into(),
//                 match current_system.clone() {
//                     Some(x) => {
//                         let mut os: OsString = x.into_os_string();
//                         os.push("/sw/bin:/usr/bin:/bin");
//                         os
//                     }
//                     None => OsString::from("/usr/local/bin:/usr/bin:/bin"),
//                 },
//             ),
//         ]);

//         Some(Self {
//             chroot_dir,
//             nix_profile,
//             current_system,
//             env,
//             nix_home: data_dir.join("nix"),
//         })
//     }

//     pub fn xdg_data_home(&self) -> &Path {
//         Path::new(
//             self.env
//                 .get(OsStr::new("XDG_DATA_HOME"))
//                 .expect("Logic error: env HashMap does not contain XDG_DATA_HOME"),
//         )
//     }

//     pub fn xdg_state_home(&self) -> &Path {
//         Path::new(
//             self.env
//                 .get(OsStr::new("XDG_STATE_HOME"))
//                 .expect("Logic error: env HashMap does not contain XDG_STATE_HOME"),
//         )
//     }

//     pub fn nixbox_bindir(&self) -> &Path {
//         Path::new(
//             self.env
//                 .get(OsStr::new("NIXBOX_BINDIR"))
//                 .expect("Logic error: env HashMap does not contain NIXBOX_BINDIR"),
//         )
//     }

//     pub fn xdg_config_home(&self) -> &Path {
//         Path::new(
//             self.env
//                 .get(OsStr::new("XDG_CONFIG_HOME"))
//                 .expect("Logic error: env HashMap does not contain XDG_CONFIG_HOME"),
//         )
//     }

//     pub fn nixbox_root(&self) -> &Path {
//         Path::new(
//             self.env
//                 .get(OsStr::new("NIXBOX_ROOT"))
//                 .expect("Logic error: env HashMap does not contain NIXBOX_ROOT"),
//         )
//     }

//     pub fn resolve_symlink(&self, path: impl AsRef<Path>) -> Result<PathBuf> {
//         let mnt = ("/nix", &self.nix_home);
//         resolve_symlink(&mnt, path)
//     }
// }

// impl From<&Service> for Config {
//     fn from(service: &Service) -> Self {
//         let mut config = Self::new(true).unwrap();
//         for (key, val) in service.env.iter() {
//             config.env.insert(key.clone(), val.clone());
//         }
//         config
//     }
// }
