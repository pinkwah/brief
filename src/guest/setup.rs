use crate::config;
use crate::guest::bind::bind;
use crate::mapper::GuestPath;

use nix::mount::{mount, MsFlags};
use nix::sched::{unshare, CloneFlags};
use nix::unistd;
use std::collections::HashSet;
use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{self, BufReader};
use std::io::{prelude::*, ErrorKind};
use std::os::unix::fs::symlink;
use std::os::unix::prelude::OsStrExt;
use std::path::Path;
use std::process::{Command, Stdio};

const NONE: Option<&'static [u8]> = None;

pub fn setup() -> Vec<(OsString, OsString)> {
    remove_chroot(config!().chroot());
    fs::create_dir_all(config!().chroot()).expect("Could not create root dir");

    let uid = unistd::getuid();
    let gid = unistd::getgid();

    unshare(CloneFlags::CLONE_NEWUSER | CloneFlags::CLONE_NEWNS)
        .unwrap_or_else(|err| panic!("unshare failed: {}", err));

    bind_guest();
    bind_tmpfiles(config!().home(), ".nix-profile/etc/tmpfiles.d");
    bind_tmpfiles(config!().chroot(), "run/current-system/etc/tmpfiles.d");
    bind_tmpfiles(config!().chroot(), "run/current-system/usr/tmpfiles.d");
    bind_common();

    let mut perms = fs::metadata(&config!().chroot()).unwrap().permissions();
    perms.set_readonly(true);
    fs::set_permissions(&config!().chroot(), perms)
        .unwrap_or_else(|err| panic!("Could not set chroot dir permissions: {}", err));

    // chroot
    unistd::chroot(config!().chroot())
        .unwrap_or_else(|err| panic!("chroot({}): {}", config!().chroot().display(), err));

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

    let mut envs: Vec<(OsString, OsString)> = vec![];
    let output = Command::new("/run/current-system/sw/bin/bash")
        .args(["-l", "-c", "env -0"])
        .stdout(Stdio::piped())
        .spawn()
        .and_then(|x| x.wait_with_output())
        .unwrap_or_else(|err| panic!("Could not log in to guest: {}", err));

    for line in output.stdout.split(|c| *c == b'\0') {
        let Some(index) = line.iter().position(|c| *c == b'=') else {
            continue;
        };
        let (key, val) = line.split_at(index);
        envs.push((
            OsStr::from_bytes(key).into(),
            OsStr::from_bytes(&val[1..]).into(),
        ));
    }
    envs
}

fn remove_chroot(path: impl AsRef<Path>) {
    let path = path.as_ref();

    let metadata = match path.metadata() {
        Ok(val) => val,
        Err(err) if err.kind() == ErrorKind::NotFound => return,
        Err(err) => panic!("Could not remove nixbox chroot: {}", err),
    };
    let mut perm = metadata.permissions();
    perm.set_readonly(false);
    fs::set_permissions(path, perm).unwrap();

    if metadata.is_dir() {
        for entry in fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            remove_chroot(entry.path());
        }
        fs::remove_dir(path).unwrap();
    } else {
        fs::remove_file(path).unwrap();
    }
}

fn create_dir<P: AsRef<Path>>(path: P) -> io::Result<()> {
    match fs::create_dir(path) {
        Err(ref x) if x.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        x => x,
    }
}

fn create_dir_all<P: AsRef<Path>>(path: P) -> io::Result<()> {
    match fs::create_dir_all(path) {
        Err(ref x) if x.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        x => x,
    }
}

fn create_symlink(source: impl AsRef<Path>, target: impl AsRef<Path>) {
    let source = source.as_ref();
    let target = target.as_ref();
    target
        .parent()
        .and_then(|parent| fs::create_dir_all(parent).ok())
        .unwrap_or_else(|| {
            panic!("Could not create parent dirs of {}", target.display());
        });
    symlink(source, target).unwrap_or_else(|err| {
        panic!(
            "Could not create symbolic link from {} to {}: {}",
            source.display(),
            target.display(),
            err
        );
    });
}

fn bind_host() {
    // bind additional directories to /
    for file_name in ["bin", "lib", "lib64", "usr", "etc"] {
        bind(Path::new("/").join(file_name), &config!().chroot());
    }
}

fn bind_guest() {
    // create /run/opengl-driver/lib in chroot, to behave like NixOS
    // (needed for nix pkgs with OpenGL or CUDA support to work)
    if let Ok(ogldir) = GuestPath::new("/nix/var/nix/opengl-driver").to_host() {
        let ogldir = ogldir.join("lib");
        if ogldir.is_dir() {
            let ogl_mount = config!().chroot().join("run/opengl-driver/lib");
            fs::create_dir_all(&ogl_mount)
                .unwrap_or_else(|err| panic!("failed to create {}: {}", &ogl_mount.display(), err));
            bind(&ogldir, &ogl_mount);
        }
    }

    // bind /etc
    create_dir(config!().chroot().join("etc")).expect("could not create etc dir");
    for file_name in ["resolv.conf", "passwd", "group", "group-", "fonts"] {
        bind(
            &Path::new("/etc").join(file_name),
            config!().chroot().join("etc"),
        );
    }
    copy_certs();

    // bind /usr
    create_dir_all(config!().chroot().join("usr/share")).expect("could not create usr/share dir");
    for file_name in ["fonts", "fontconfig", "icons"] {
        bind(
            &Path::new("/usr/share").join(file_name),
            config!().chroot().join("usr/share"),
        );
    }

    let sysroot = GuestPath::new("/nix/var/nix/profiles/system");
    // current-system -> /run/current-system
    create_symlink(
        sysroot.to_host().unwrap(),
        config!().chroot().join("run/current-system"),
    );

    // current-system/sw/bin/sh -> /bin/sh
    create_symlink(
        sysroot.join("sw/bin/sh").to_host().unwrap(),
        config!().chroot().join("bin/sh"),
    );

    // current-system/sw/bin/env -> /usr/bin/env
    create_symlink(
        sysroot.join("sw/bin/env").to_host().unwrap(),
        config!().chroot().join("usr/bin/env"),
    );

    let etcdir = sysroot.join("etc").to_host().unwrap();
    for entry in fs::read_dir(&etcdir).unwrap() {
        let entry = entry.unwrap();
        let target = config!().chroot().join("etc").join(entry.file_name());

        let entrypath =
            Path::new("/run/current-system/etc").join(entry.path().strip_prefix(&etcdir).unwrap());

        if !target.exists() {
            create_symlink(&entrypath, &target);
        }
    }
}

fn listdir(path: impl AsRef<Path>) {
    let path = path.as_ref();
    eprintln!("Listing {}", path.display());
    for entry in fs::read_dir(path).unwrap() {
        let entry = entry.unwrap();
        if let Ok(link) = fs::read_link(entry.path()) {
            eprintln!("\t{} -> {}", entry.path().display(), link.display());
        } else {
            eprintln!("\t{}", entry.path().display());
        }
    }
    eprintln!("");
}

fn bind_common() {
    // mount the store
    let nix_mount = config!().chroot().join("nix");
    fs::create_dir(&nix_mount)
        .unwrap_or_else(|err| panic!("failed to create {}: {}", &nix_mount.display(), err));
    mount(
        Some(config!().nix()),
        &nix_mount,
        Some("none"),
        MsFlags::MS_BIND | MsFlags::MS_REC,
        NONE,
    )
    .unwrap_or_else(|err| {
        panic!(
            "failed to bind mount {} to /nix: {}",
            config!().nix().display(),
            err
        )
    });

    // bind nixbox "/home"
    let home_mount = env::var("HOME").unwrap();
    let home_mount = config!()
        .chroot()
        .join(Path::new(&home_mount).strip_prefix("/").unwrap());
    fs::create_dir(&home_mount)
        .unwrap_or_else(|err| panic!("failed to create {}: {}", &home_mount.display(), err));
    mount(
        Some(config!().home()),
        &home_mount,
        Some("none"),
        MsFlags::MS_BIND | MsFlags::MS_REC,
        NONE,
    )
    .unwrap_or_else(|err| {
        panic!(
            "failed to bind mount {} to /home: {}",
            config!().home().display(),
            err
        )
    });

    // bind the real host to /run/host
    fs::create_dir_all(&config!().chroot().join("run/host")).unwrap();
    mount(
        Some("/"),
        &config!().chroot().join("run/host"),
        Some("none"),
        MsFlags::MS_BIND | MsFlags::MS_REC,
        NONE,
    )
    .unwrap();

    // bind directories from /
    for file_name in ["dev", "proc", "var", "run", "opt", "srv", "sys", "tmp"] {
        bind(Path::new("/").join(file_name), &config!().chroot());
    }
}

fn bind_tmpfiles(base: impl AsRef<Path>, path: impl AsRef<GuestPath>) {
    let Ok(path) = path.as_ref().to_host() else {
        return;
    };
    let Ok(dir) = fs::read_dir(path) else {
        return;
    };

    for entry in dir {
        let path = entry.unwrap().path();
        let Ok(path) = GuestPath::new(&path).to_host() else {
            continue;
        };
        if !path.is_file() {
            continue;
        }
        let file = fs::File::open(path).unwrap();
        let reader = BufReader::new(file);

        for line in reader.lines() {
            let line = line.unwrap();
            let vec = line.split_ascii_whitespace().collect::<Vec<_>>();
            if let ["L+", target, "-", "-", "-", "-", source] = vec.as_slice() {
                let Some(target) = target.strip_prefix('/') else {
                    continue;
                };
                let target = config!().chroot().join(target);
                fs::create_dir_all(target.parent().unwrap_or(Path::new("/"))).unwrap();
                create_symlink(Path::new(source), &target);
            }
        }
    }
}

fn copy_certs() {
    let paths = [
        "ssl/certs/ca-certificates.crt",
        "ssl/certs/ca-bundle.crt",
        "pki/tls/certs/ca-bundle.crt",
    ]
    .map(Path::new);

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
        let targetpath = config!().chroot().join("etc").join(path);
        fs::create_dir_all(targetpath.parent().unwrap()).unwrap_or(());
        fs::copy(&sourcepath, &targetpath).unwrap_or(0);
    }
}
