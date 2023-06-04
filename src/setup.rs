use crate::bind::bind;
use crate::util::resolve_symlink;

use crate::config::Config;
use nix::mount::{mount, MsFlags};
use nix::sched::{unshare, CloneFlags};
use nix::unistd;
use std::collections::HashSet;
use std::env;
use std::fs;
use std::io::prelude::*;
use std::io::{self, BufReader};
use std::os::unix::fs::symlink;
use std::path::Path;

const NONE: Option<&'static [u8]> = None;

pub fn setup(config: &Config) {
    fs::create_dir_all(&config.chroot_dir).expect("Could not create root dir");

    let uid = unistd::getuid();
    let gid = unistd::getgid();

    unshare(CloneFlags::CLONE_NEWNS | CloneFlags::CLONE_NEWUSER | CloneFlags::CLONE_NEWUTS)
        .expect("unshare failed");

    if let Some(nix_profile_dir) = &config.nix_profile {
        bind_nix_profile(&config.chroot_dir, &config.nix_home, config.nixbox_root());
        bind_tmpfiles(nix_profile_dir);

        if let Some(current_system) = &config.current_system {
            bind_tmpfiles(current_system);
        }
    } else {
        bind_host(&config.chroot_dir);
    }
    bind_common(&config.nix_home, &config.chroot_dir);

    let mut perms = fs::metadata(&config.chroot_dir).unwrap().permissions();
    perms.set_readonly(true);
    fs::set_permissions(&config.chroot_dir, perms)
        .unwrap_or_else(|err| panic!("Could not set chroot dir permissions: {}", err));

    let cwd = env::current_dir().expect("cannot get current working directory");

    // chroot
    unistd::chroot(&config.chroot_dir)
        .unwrap_or_else(|err| panic!("chroot({}): {}", config.chroot_dir.display(), err));

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

    // restore cwd
    env::set_current_dir(&cwd)
        .unwrap_or_else(|_| panic!("cannot restore working directory {}", cwd.display()));
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

fn create_symlink(source: &Path, target: &Path) {
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

fn bind_host(chroot_dir: &Path) {
    // bind additional directories to /
    for file_name in ["bin", "lib", "lib64", "usr", "etc"] {
        bind(Path::new("/").join(file_name), chroot_dir);
    }
}

fn bind_nix_profile(chroot_dir: &Path, nix_dir: &Path, nixbox_root: &Path) {
    // create /run/opengl-driver/lib in chroot, to behave like NixOS
    // (needed for nix pkgs with OpenGL or CUDA support to work)
    if let Ok(ogldir) = resolve_symlink(
        &(&Path::new("/nix"), &nix_dir),
        nix_dir.join("var/nix/opengl-driver"),
    ) {
        let ogldir = ogldir.join("lib");
        if ogldir.is_dir() {
            let ogl_mount = chroot_dir.join("run/opengl-driver/lib");
            fs::create_dir_all(&ogl_mount)
                .unwrap_or_else(|err| panic!("failed to create {}: {}", &ogl_mount.display(), err));
            bind(&ogldir, &ogl_mount);
        }
    }

    // bind /etc
    create_dir(chroot_dir.join("etc")).expect("could not create etc dir");
    for file_name in ["resolv.conf", "passwd", "group", "group-", "fonts"] {
        bind(&Path::new("/etc").join(file_name), chroot_dir.join("etc"));
    }
    copy_certs(chroot_dir);

    // bind /usr
    create_dir_all(chroot_dir.join("usr/share")).expect("could not create usr/share dir");
    for file_name in ["fonts", "fontconfig", "icons"] {
        bind(
            &Path::new("/usr/share").join(file_name),
            chroot_dir.join("usr/share"),
        );
    }

    if let Ok(sysroot) = resolve_symlink(&(&Path::new("/nix"), &nix_dir), nixbox_root) {
        // current-system -> /run/current-system
        create_symlink(&sysroot, &chroot_dir.join("run/current-system"));

        // current-system/sw/bin/sh -> /bin/sh
        create_symlink(&sysroot.join("sw/bin/sh"), &chroot_dir.join("bin/sh"));

        // current-system/sw/bin/env -> /usr/bin/env
        create_symlink(&sysroot.join("sw/bin/env"), &chroot_dir.join("usr/bin/env"));

        let etcdir = resolve_symlink(&(&Path::new("/nix"), &nix_dir), sysroot.join("etc")).unwrap();
        for entry in fs::read_dir(etcdir).unwrap() {
            let entry = entry.unwrap();
            let target = chroot_dir.join("etc").join(entry.file_name());

            if !target.exists() {
                create_symlink(&entry.path(), &target);
            }
        }
    }
}

fn bind_common(nix_dir: &Path, chroot_dir: &Path) {
    // mount the store
    let nix_mount = chroot_dir.join("nix");
    fs::create_dir(&nix_mount)
        .unwrap_or_else(|err| panic!("failed to create {}: {}", &nix_mount.display(), err));
    mount(
        Some(nix_dir),
        &nix_mount,
        Some("none"),
        MsFlags::MS_BIND | MsFlags::MS_REC,
        NONE,
    )
    .unwrap_or_else(|err| {
        panic!(
            "failed to bind mount {} to /nix: {}",
            nix_dir.display(),
            err
        )
    });

    // bind directories from /
    for file_name in [
        "dev", "proc", "home", "var", "run", "opt", "srv", "sys", "tmp",
    ] {
        bind(Path::new("/").join(file_name), chroot_dir);
    }
}

fn bind_tmpfiles(path: &Path) {
    let Ok(dir) = fs::read_dir(path.join("lib/tmpfiles.d")) else { return; };

    for entry in dir {
        let path = entry.unwrap().path();
        if !path.is_file() {
            continue;
        }
        let file = fs::File::open(path).unwrap();
        let reader = BufReader::new(file);

        for line in reader.lines() {
            let line = line.unwrap();
            let vec = line.split_ascii_whitespace().collect::<Vec<_>>();
            if let ["L+", target, "-", "-", "-", "-", source] = vec.as_slice() {
                fs::create_dir_all(Path::new(target).parent().unwrap_or(Path::new("/"))).unwrap();
                create_symlink(Path::new(source), Path::new(target));
            }
        }
    }
}

fn copy_certs(chroot_dir: &Path) {
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
        let targetpath = chroot_dir.join("etc").join(path);
        fs::create_dir_all(targetpath.parent().unwrap()).unwrap_or(());
        fs::copy(&sourcepath, &targetpath).unwrap_or(0);
    }
}
