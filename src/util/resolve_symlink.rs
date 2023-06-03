use std::env::current_dir;
use std::fs;
use std::io::Result;
use std::path::{Path, PathBuf};

pub fn resolve_symlink(
    mapping: &(impl AsRef<Path>, impl AsRef<Path>),
    path: impl AsRef<Path>,
) -> Result<PathBuf> {
    let mount = mapping.0.as_ref();
    let onto = mapping.1.as_ref();
    let path = path.as_ref();
    let mut path = if path.is_absolute() {
        path.to_owned()
    } else {
        current_dir()?.join(path)
    };

    loop {
        if path.symlink_metadata().is_err() {
            for parent in path.ancestors().skip(1) {
                if !parent.is_symlink() {
                    continue;
                }

                let resolved = resolve_symlink(mapping, parent)?;
                path = resolved.join(path.strip_prefix(parent).unwrap());
                break;
            }
        }

        if !path.is_symlink() {
            return path.canonicalize();
        }
        let mut target = fs::read_link(&path)?;

        let dir = match path.parent() {
            Some(x) => x.to_path_buf(),
            _ => PathBuf::from("/"),
        };
        if !target.is_absolute() {
            target = dir.join(target);
        }

        path = if target.starts_with(mount) {
            onto.join(
                target
                    .strip_prefix(mount)
                    .expect("strip mount prefix from target"),
            )
        } else {
            PathBuf::from(&target)
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::os::unix::fs::symlink;
    use testdir::testdir;

    #[test]
    fn it_resolves_normal_symlinks() {
        assert!(Path::new("/bin/sh").is_absolute());

        let expect = Path::new("/bin/sh").canonicalize().unwrap();
        let actual = resolve_symlink(&("/not-exist", "/mnt"), "/bin/sh").unwrap();

        assert_eq!(actual, expect);
    }

    #[test]
    fn it_resolves_simple_symlink() {
        let path: PathBuf = testdir!();
        let expect = path.join("file");

        File::create(&expect).unwrap();
        symlink("/fake-directory/file", path.join("symlink")).unwrap();

        // Cannot resolve symlink
        assert!(path.join("symlink").canonicalize().is_err());

        let actual = resolve_symlink(&("/fake-directory", &path), &path.join("symlink")).unwrap();
        assert_eq!(expect, actual);
    }

    #[test]
    fn it_resolves_through_subdirectories() {
        let path: PathBuf = testdir!();
        let expect = path.join("file");

        File::create(&expect).unwrap();

        // 'dir/file' -> 'file'
        fs::create_dir(path.join("dir")).unwrap();
        symlink("/fake-directory/file", path.join("dir/file")).unwrap();

        // 'symlink' -> 'dir'
        symlink("/fake-directory/dir", path.join("symlink")).unwrap();

        // Cannot resolve symlink
        assert!(path.join("symlink/file").canonicalize().is_err());

        let actual =
            resolve_symlink(&("/fake-directory", &path), &path.join("symlink/file")).unwrap();
        assert_eq!(expect, actual);
    }

    #[test]
    fn it_errors_when_file_not_found() {
        let path: PathBuf = testdir!();

        fs::create_dir(path.join("dir")).unwrap();
        symlink("/fake-directory/dir", path.join("symlink")).unwrap();

        let expect = path.join("symlink/file").canonicalize().unwrap_err();
        let actual =
            resolve_symlink(&("/fake-directory", &path), &path.join("symlink/file")).unwrap_err();
        assert_eq!(expect.kind(), actual.kind());
    }
}
