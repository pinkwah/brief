use crate::resolve_symlink::resolve_symlink;
use crate::bind::bind;

use nix::mount::{mount, MsFlags};
use nix::sched::{unshare, CloneFlags};
use nix::unistd::{self, Gid, Uid};
use std::collections::HashSet;
use std::env;
use std::fs;
use std::io::prelude::*;
use std::io::{self, BufReader};
use std::os::unix::fs::symlink;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process;
use std::string::String;

const NONE: Option<&'static [u8]> = None;

fn create_dir<P: AsRef<Path>>(path: P) -> io::Result<()> {
    match fs::create_dir(path) {
        Err(ref x) if x.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        x => x
    }
}

fn create_dir_all<P: AsRef<Path>>(path: P) -> io::Result<()> {
    match fs::create_dir_all(path) {
        Err(ref x) if x.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        x => x
    }
}

pub struct RunChroot<'a> {
    rootdir: &'a Path,
    nixdir: &'a Path,
}

impl<'a> RunChroot<'a> {
    pub fn new(rootdir: &'a Path, nixdir: &'a Path) -> Self {
        Self {
            rootdir,
            nixdir,
        }
    }

    fn symlink(&self, source: &Path, target: &Path) {
        let target = self.rootdir.join(target);
        target
            .parent()
            .and_then(|parent| fs::create_dir_all(parent).ok())
            .unwrap_or_else(|| {
                panic!("Could not create parent dirs of {}", target.display());
            });
        symlink(source, &target).unwrap_or_else(|err| {
            panic!(
                "Could not create symbolic link from {} to {}: {}",
                source.display(),
                target.display(),
                err
            );
        });
    }

    fn resolve(&self, path: &PathBuf) -> io::Result<PathBuf> {
        resolve_symlink("/nix", self.nixdir, &path)
    }

    fn nix_profile_path(&self) -> io::Result<PathBuf> {
        let home = env::var_os("HOME").ok_or_else(|| io::ErrorKind::NotFound)?;
        self.resolve(&Path::new(&home).join(".nix-profile"))
    }

    fn bind_root<P: AsRef<Path>>(&self, path: P) {
        let path = Path::new("/").join(path.as_ref());
        bind(&path, self.rootdir);
    }

    fn bind<P: AsRef<Path>>(&self, path: &Path, reldir: P) {
        bind(path, self.rootdir.join(reldir));
    }

    pub fn bind_defaults(&self) {
        // mount the store
        let nix_mount = self.rootdir.join("nix");
        fs::create_dir(&nix_mount)
            .unwrap_or_else(|err| panic!("failed to create {}: {}", &nix_mount.display(), err));
        mount(
            Some(self.nixdir),
            &nix_mount,
            Some("none"),
            MsFlags::MS_BIND | MsFlags::MS_REC,
            NONE,
        )
        .unwrap_or_else(|err| {
            panic!(
                "failed to bind mount {} to /nix: {}",
                self.nixdir.display(),
                err
            )
        });

        // bind directories from /
        for file_name in ["dev", "proc", "home", "var", "run", "opt", "srv", "sys", "tmp"] {
            self.bind_root(file_name);
        }

        // bind /etc
        create_dir(self.rootdir.join("etc")).expect("could not create etc dir");
        for file_name in ["resolv.conf", "passwd", "group", "group-", "fonts"] {
            self.bind(&Path::new("/etc").join(file_name), "etc");
        }
        self.copy_certs();

        // bind /usr
        create_dir_all(self.rootdir.join("usr/share")).expect("could not create usr/share dir");
        for file_name in ["fonts", "fontconfig", "icons"] {
            self.bind(&Path::new("/usr/share").join(file_name), "usr/share");
        }
    }

    pub fn bind_host(&self) {
        // bind additional directories to /
        for file_name in ["bin", "lib", "lib64", "usr"] {
            self.bind_root(file_name);
        }
    }

    pub fn bind_nix_profile(&self) {
        let nix_profile_path = self.nix_profile_path().expect("Could not find user's .nix-profile");

        // /bin/sh
        self.symlink(&nix_profile_path.join("bin/sh"), &PathBuf::from("bin/sh"));
        self.symlink(
            &nix_profile_path.join("bin/bash"),
            &PathBuf::from("bin/bash"),
        );

        // /usr/bin/env
        self.symlink(
            &nix_profile_path.join("bin/env"),
            &PathBuf::from("usr/bin/env"),
        );

        // let etcuserdir = self.resolve(&nix_profile_path.join("etc/pinkwah")).unwrap();
        // create_dir(self.rootdir.join("etc")).expect("Could not create <rootdir>/etc");
        // for entry in fs::read_dir(&etcuserdir)
        //     .expect("failed to list user's nix-profile etc/pinkwah directory")
        // {
        //     let entry =
        //         entry.expect("error while listing from user's nix-profile etc/pinkwah directory");
        //     symlink(
        //         entry.path(),
        //         self.rootdir.join("etc").join(entry.file_name()),
        //     )
        //     .unwrap();
        // }

        // create /run/opengl-driver/lib in chroot, to behave like NixOS
        // (needed for nix pkgs with OpenGL or CUDA support to work)
        if let Ok(ogldir) = self.resolve(&self.nixdir.join("var/nix/opengl-driver")) {
            let ogldir = ogldir.join("lib");
            if ogldir.is_dir() {
                let ogl_mount = self.rootdir.join("run/opengl-driver/lib");
                fs::create_dir_all(&ogl_mount).unwrap_or_else(|err| {
                    panic!("failed to create {}: {}", &ogl_mount.display(), err)
                });
                bind(&ogldir, &ogl_mount);
            }
        }
    }

    pub fn unshare(&self) -> (Uid, Gid) {
        fs::create_dir_all(self.rootdir).expect("Could not create root dir");

        let uid = unistd::getuid();
        let gid = unistd::getgid();

        unshare(CloneFlags::CLONE_NEWNS | CloneFlags::CLONE_NEWUSER).expect("unshare failed");

        (uid, gid)
    }

    pub fn exec(&self, uid: Uid, gid: Gid, cmd: &str, args: &[String]) {
        let cwd = env::current_dir().expect("cannot get current working directory");

        // chroot
        unistd::chroot(self.rootdir)
            .unwrap_or_else(|err| panic!("chroot({}): {}", self.rootdir.display(), err));

        env::set_current_dir("/").expect("cannot change directory to /");

        // fixes issue #1 where writing to /proc/self/gid_map fails
        // see user_namespaces(7) for more documentation
        let _ = fs::File::create("/proc/self/setgroups").and_then(|mut file| file.write_all(b"deny"));

        fs::File::create("/proc/self/uid_map")
            .expect("failed to open /proc/self/uid_map")
            .write_all(format!("{} {} 1", uid, uid).as_bytes())
            .expect("failed to write new uid mapping to /proc/self/uid_map");

        fs::File::create("/proc/self/gid_map")
            .expect("failed to open /proc/self/gid_map")
            .write_all(format!("{} {} 1", gid, gid).as_bytes())
            .expect("failed to write new gid mapping to /proc/self/gid_map");

        self.tmpfiles();

        // restore cwd
        env::set_current_dir(&cwd)
            .unwrap_or_else(|_| panic!("cannot restore working directory {}", cwd.display()));

        let new_path = if let Ok(path) = self.nix_profile_path() {
            format!("{}/.nix-profile/bin:/usr/bin:/bin", env::var("HOME").unwrap())
        } else {
            "/usr/bin:/bin".into()
        };
        let mut command = process::Command::new(cmd);
        command
            .args(args)
            .env_clear()
            .env("NIX_CONF_DIR", "/nix/etc/nix")
            .env("SHELL", "/var/home/zohar/.nix-profile/bin/fish")
            .env("PATH", new_path);

        for var in [
            "DESKTOP_SESSION",
            "DISPLAY",
            "HOME",
            "LANG",
            "TERM",
            "USER",
            "WAYLAND_DISPLAY",
            "XAUTHORITY",
            "XAUTHORITY",
            "XDG_CONFIG_HOME",
            "XDG_CURRENT_DESKTOP",
            "XDG_DATA_DIRS",
            "XDG_DATA_HOME",
            "XDG_RUNTIME_DIR",
            "XDG_SESSION_DESKTOP",
            "XDG_SESSION_TYPE",
        ]
        .iter()
        {
            if let Ok(val) = env::var(var) {
                command.env(var, val);
            }
        }

        let err = command.exec();

        eprintln!("failed to execute {}: {}", &cmd, err);
        process::exit(1);
    }

    fn copy_certs(&self) {
        let paths = [
            "ssl/certs/ca-certificates.crt",
            "ssl/certs/ca-bundle.crt",
            "pki/tls/certs/ca-bundle.crt",
        ]
        .map(|path| Path::new(path));

        let found_paths = paths
            .iter()
            .map(|path| Path::new("/etc").join(path))
            .filter(|path| path.is_file())
            .map(|path| path.canonicalize().unwrap())
            .collect::<HashSet<_>>();

        if found_paths.is_empty() {
            eprintln!("Warning: No SSL certificate bundles found on host system");
            return;
        }

        if found_paths.len() >= 2 {
            eprintln!(
                "Warning: Found {} SSL certificate bundle candidates. Picking the first one.",
                found_paths.len()
            );
        }
        let found_paths = found_paths.iter().collect::<Vec<_>>();

        let sourcepath = Path::new("/etc").join(found_paths[0]);
        for path in paths.iter() {
            let targetpath = self.rootdir.join("etc").join(path);
            fs::create_dir_all(targetpath.parent().unwrap()).unwrap_or(());
            fs::copy(&sourcepath, &targetpath).unwrap_or(0);
        }
    }

    fn tmpfiles(&self) {
        if let Ok(profiledir) = self.nix_profile_path() {
            for entry in fs::read_dir(profiledir.join("lib/tmpfiles.d")).unwrap() {
                let path = entry.unwrap().path();
                if !path.is_file() {
                    continue;
                }
                let file = fs::File::open(path).unwrap();
                let reader = BufReader::new(file);

                for line in reader.lines() {
                    let line = line.unwrap();
                    let vec = line.split_ascii_whitespace().collect::<Vec<_>>();
                    match vec.as_slice() {
                        ["L+", target, "-", "-", "-", "-", source] => {
                            fs::create_dir_all(
                                Path::new(target).parent().unwrap_or(Path::new("/")),
                            )
                            .unwrap();
                            symlink(Path::new(source), Path::new(target)).unwrap();
                        }
                        _ => (),
                    };
                }
            }
        }
    }
}
