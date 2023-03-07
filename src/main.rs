use nix::mount::{mount, MsFlags};
use nix::sched::{unshare, CloneFlags};
use nix::sys::signal::{kill, Signal};
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::{self, fork, ForkResult};
use std::collections::HashSet;
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::io::prelude::*;
use std::io::{self, BufReader};
use std::os::unix::fs::symlink;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process;
use std::string::String;

mod mkdtemp;

const NONE: Option<&'static [u8]> = None;

fn bind_mount(source: &Path, dest: &Path) {
    if let Err(e) = mount(
        Some(source),
        dest,
        Some("none"),
        MsFlags::MS_BIND | MsFlags::MS_REC | MsFlags::MS_PRIVATE,
        NONE,
    ) {
        eprintln!(
            "failed to bind mount {} to {}: {}",
            source.display(),
            dest.display(),
            e
        );
    }
}

pub struct RunChroot<'a> {
    rootdir: &'a Path,
    nixdir: &'a Path,
}

impl<'a> RunChroot<'a> {
    fn new(rootdir: &'a Path, nixdir: &'a Path) -> Self {
        Self { rootdir, nixdir }
    }

    fn bind_mount_directory(&self, entry: &fs::DirEntry, root: &Path) {
        let mountpoint = self.rootdir.join(entry.file_name());

        // if the destination doesn't exist we can proceed as normal
        if !mountpoint.exists() {
            if let Err(e) = fs::create_dir(&mountpoint) {
                if e.kind() != io::ErrorKind::AlreadyExists {
                    panic!("failed to create {}: {}", &mountpoint.display(), e);
                }
            }

            bind_mount(&entry.path(), &mountpoint)
        } else {
            // otherwise, if the dest is also a dir, we can recurse into it
            // and mount subdirectory siblings of existing paths
            if mountpoint.is_dir() {
                let dir = fs::read_dir(entry.path()).unwrap_or_else(|err| {
                    panic!("failed to list dir {}: {}", entry.path().display(), err)
                });

                let child = RunChroot::new(&mountpoint, self.nixdir);
                for entry in dir {
                    let entry = entry.expect("error while listing subdir");
                    child.bind_mount_direntry(&entry, &root);
                }
            }
        }
    }

    fn bind_mount_file(&self, entry: &fs::DirEntry, path: &Path) {
        let mountpoint = self.rootdir.join(path).join(entry.file_name());
        if mountpoint.exists() {
            return;
        }
        fs::File::create(&mountpoint)
            .unwrap_or_else(|err| panic!("failed to create {}: {}", &mountpoint.display(), err));

        bind_mount(&entry.path(), &mountpoint)
    }

    fn mirror_symlink(&self, entry: &fs::DirEntry, path: &Path) {
        let link_path = self.rootdir.join(path).join(entry.file_name());
        if link_path.exists() {
            return;
        }
        let path = entry.path();
        let target = fs::read_link(&path)
            .unwrap_or_else(|err| panic!("failed to resolve symlink {}: {}", &path.display(), err));
        symlink(&target, &link_path).unwrap_or_else(|_| {
            panic!(
                "failed to create symlink {} -> {}",
                &link_path.display(),
                &target.display()
            )
        });
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
        let mut path = path.to_path_buf();
        let mut cwd = env::current_dir()?;

        loop {
            if !path.is_absolute() {
                path = cwd.join(path);
            }
            if !path.is_symlink() {
                return Ok(path.to_path_buf());
            }
            let target = fs::read_link(&path)?.to_str().unwrap().to_owned();

            cwd = match path.parent() {
                Some(x) => x.to_path_buf(),
                _ => PathBuf::from("/"),
            };
            path = if target.starts_with("/nix/") {
                self.nixdir.join(PathBuf::from(&target["/nix/".len()..]))
            } else {
                PathBuf::from(&target)
            };
        }
    }

    fn bind_mount_direntry(&self, entry: &fs::DirEntry, root: &Path) {
        let path = entry.path();
        let stat = entry
            .metadata()
            .unwrap_or_else(|err| panic!("cannot get stat of {}: {}", path.display(), err));

        if stat.is_dir() {
            self.bind_mount_directory(entry, root);
        } else if stat.is_file() {
            self.bind_mount_file(entry, root);
        } else if stat.file_type().is_symlink() {
            self.mirror_symlink(entry, root);
        }
    }

    fn run_chroot(&self, cmd: &str, args: &[String]) {
        let cwd = env::current_dir().expect("cannot get current working directory");

        let uid = unistd::getuid();
        let gid = unistd::getgid();

        unshare(CloneFlags::CLONE_NEWNS | CloneFlags::CLONE_NEWUSER).expect("unshare failed");

        // create /run/opengl-driver/lib in chroot, to behave like NixOS
        // (needed for nix pkgs with OpenGL or CUDA support to work)
        if let Ok(ogldir) = self.resolve(&self.nixdir.join("var/nix/opengl-driver")) {
            let ogldir = ogldir.join("lib");
            if ogldir.is_dir() {
                let ogl_mount = self.rootdir.join("run/opengl-driver/lib");
                fs::create_dir_all(&ogl_mount).unwrap_or_else(|err| {
                    panic!("failed to create {}: {}", &ogl_mount.display(), err)
                });
                bind_mount(&ogldir, &ogl_mount);
            }
        }

        // bind the rest of / stuff into rootdir
        let excepts = ["nix", "bin", "usr", "lib", "lib64", "etc"].map(|x| OsStr::new(x));
        let nix_root = PathBuf::from("/");
        let dir = fs::read_dir(&nix_root).expect("failed to list /nix directory");
        for entry in dir {
            let entry = entry.expect("error while listing from /nix directory");
            // do not bind mount an existing nix installation
            if excepts.iter().any(|except| entry.file_name() == *except) {
                continue;
            }
            self.bind_mount_direntry(&entry, Path::new(""));
        }

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

        let nix_profile_path = self
            .resolve(
                &PathBuf::from(
                    env::var("HOME")
                        .unwrap_or_else(|err| panic!("HOME environment variable not set: {}", err)),
                )
                .join(".nix-profile"),
            )
            .unwrap();

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

        // /etc
        let etcdir = nix_profile_path.join("etc");
        fs::create_dir(self.rootdir.join("etc")).expect("failed to create /etc directory");
        for entry in
            fs::read_dir(&etcdir).expect("failed to list user's nix-profile etc/ directory")
        {
            let entry = entry.expect("error while listing from user's nix-profile etc/ directory");
            symlink(
                entry.path(),
                self.rootdir.join("etc").join(entry.file_name()),
            )
            .unwrap();
        }

        let etcuserdir = self.resolve(&nix_profile_path.join("etc/pinkwah")).unwrap();
        for entry in fs::read_dir(&etcuserdir)
            .expect("failed to list user's nix-profile etc/pinkwah directory")
        {
            let entry =
                entry.expect("error while listing from user's nix-profile etc/pinkwah directory");
            symlink(
                entry.path(),
                self.rootdir.join("etc").join(entry.file_name()),
            )
            .unwrap();
        }

        self.copy_certs();
        let etcentries = ["resolv.conf", "passwd", "group", "group-"].map(|name| OsStr::new(name));
        for entry in fs::read_dir("/etc").unwrap() {
            let entry = entry.expect("error while listing from /etc directory");
            if etcentries.iter().any(|name| entry.file_name() == *name) {
                self.bind_mount_direntry(&entry, Path::new("etc"));
            }
        }

        // chroot
        unistd::chroot(self.rootdir)
            .unwrap_or_else(|err| panic!("chroot({}): {}", self.rootdir.display(), err));

        env::set_current_dir("/").expect("cannot change directory to /");

        // fixes issue #1 where writing to /proc/self/gid_map fails
        // see user_namespaces(7) for more documentation
        if let Ok(mut file) = fs::File::create("/proc/self/setgroups") {
            let _ = file.write_all(b"deny");
        }

        let mut uid_map =
            fs::File::create("/proc/self/uid_map").expect("failed to open /proc/self/uid_map");
        uid_map
            .write_all(format!("{} {} 1", uid, uid).as_bytes())
            .expect("failed to write new uid mapping to /proc/self/uid_map");

        let mut gid_map =
            fs::File::create("/proc/self/gid_map").expect("failed to open /proc/self/gid_map");
        gid_map
            .write_all(format!("{} {} 1", gid, gid).as_bytes())
            .expect("failed to write new gid mapping to /proc/self/gid_map");

        self.tmpfiles();

        // restore cwd
        env::set_current_dir(&cwd)
            .unwrap_or_else(|_| panic!("cannot restore working directory {}", cwd.display()));

        let new_path = format!("{}/bin:/usr/bin:/bin", nix_profile_path.display());
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
            fs::create_dir_all(targetpath.parent().unwrap());
            fs::copy(&sourcepath, &targetpath);
        }
    }

    fn tmpfiles(&self) {
        let homedir = env::var("HOME").unwrap();
        let profiledir = self
            .resolve(&Path::new(&homedir).join(".nix-profile"))
            .unwrap();
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
                        fs::create_dir_all(Path::new(target).parent().unwrap_or(Path::new("/")))
                            .unwrap();
                        symlink(Path::new(source), Path::new(target)).unwrap();
                    }
                    _ => (),
                };
            }
        }
    }
}

fn wait_for_child(rootdir: &Path, child_pid: unistd::Pid) -> ! {
    let mut exit_status = 1;
    loop {
        match waitpid(child_pid, Some(WaitPidFlag::WUNTRACED)) {
            Ok(WaitStatus::Signaled(child, Signal::SIGSTOP, _)) => {
                let _ = kill(unistd::getpid(), Signal::SIGSTOP);
                let _ = kill(child, Signal::SIGCONT);
            }
            Ok(WaitStatus::Signaled(_, signal, _)) => {
                kill(unistd::getpid(), signal).unwrap_or_else(|err| {
                    panic!("failed to send {} signal to our self: {}", signal, err)
                });
            }
            Ok(WaitStatus::Exited(_, status)) => {
                exit_status = status;
                break;
            }
            Ok(what) => {
                eprintln!("unexpected wait event happend: {:?}", what);
                break;
            }
            Err(e) => {
                eprintln!("waitpid failed: {}", e);
                break;
            }
        };
    }

    fs::remove_dir_all(rootdir)
        .unwrap_or_else(|err| panic!("cannot remove tempdir {}: {}", rootdir.display(), err));

    process::exit(exit_status);
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: {} <nixpath> <command>\n", args[0]);
        process::exit(1);
    }

    let rootdir = mkdtemp::mkdtemp("nix-chroot.XXXXXX")
        .unwrap_or_else(|err| panic!("failed to create temporary directory: {}", err));

    let nixdir = fs::canonicalize(&args[1])
        .unwrap_or_else(|err| panic!("failed to resolve nix directory {}: {}", &args[1], err));

    match unsafe { fork() } {
        Ok(ForkResult::Parent { child, .. }) => wait_for_child(&rootdir, child),
        Ok(ForkResult::Child) => RunChroot::new(&rootdir, &nixdir).run_chroot(&args[2], &args[3..]),
        Err(e) => {
            eprintln!("fork failed: {}", e);
        }
    };
}
