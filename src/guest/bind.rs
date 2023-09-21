use nix::mount::{mount, MsFlags};
use std::fs;
use std::path::Path;

pub fn bind<P: AsRef<Path>, Q: AsRef<Path>>(source: P, targetdir: Q) {
    let source = source.as_ref();
    let target = targetdir.as_ref().join(source.file_name().unwrap());
    let stat = source
        .metadata()
        .unwrap_or_else(|err| panic!("Could not stat '{}': {}", source.display(), err));

    if stat.is_dir() {
        bind_dir(source, &target);
    } else if stat.file_type().is_symlink() {
        bind_symlink(source, &target);
    } else {
        bind_file(source, &target);
    }
}

fn bind_mount(source: &Path, dest: &Path) {
    const NONE: Option<&'static [u8]> = None;

    mount(
        Some(source),
        dest,
        Some("none"),
        MsFlags::MS_BIND | MsFlags::MS_REC | MsFlags::MS_PRIVATE,
        NONE,
    )
    .unwrap_or_else(|err| {
        panic!(
            "Could not mount '{}' to '{}': {}",
            source.display(),
            dest.display(),
            err
        )
    });
}

fn bind_dir(path: &Path, target: &Path) {
    if !target.exists() {
        fs::create_dir_all(target).unwrap_or_else(|err| {
            panic!("Could not create directory '{}': {}", target.display(), err)
        });
        bind_mount(path, target);
    } else if target.is_dir() {
        for entry in fs::read_dir(path)
            .unwrap_or_else(|err| panic!("Could not list dir '{}': {}", target.display(), err))
        {
            let entry = entry.unwrap();
            bind(&entry.path(), target);
        }
    }
}

fn bind_file(path: &Path, target: &Path) {
    fs::File::create(target)
        .unwrap_or_else(|err| panic!("Could not create file '{}': {}", target.display(), err));
    bind_mount(path, target);
}

fn bind_symlink(path: &Path, target: &Path) {
    panic!(
        "Cannot symlink '{}' to '{}': Not implemented",
        path.display(),
        target.display()
    );
    // let Ok(file_name) = path.file_name() else { return; };
    // let path = fs::read_link(&path)?;
    // fs::symlink(&path, )
}
