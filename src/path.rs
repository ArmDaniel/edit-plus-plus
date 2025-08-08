// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Path related helpers.

use std::ffi::{OsStr, OsString};
use std::path::{Component, MAIN_SEPARATOR_STR, Path, PathBuf};

/// Normalizes a given path by removing redundant components.
/// The given path must be absolute (e.g. by joining it with the current working directory).
pub fn normalize(path: &Path) -> PathBuf {
    let mut res = PathBuf::with_capacity(path.as_os_str().as_encoded_bytes().len());
    let mut root_len = 0;

    for component in path.components() {
        match component {
            Component::Prefix(p) => res.push(p.as_os_str()),
            Component::RootDir => {
                res.push(OsStr::new(MAIN_SEPARATOR_STR));
                root_len = res.as_os_str().as_encoded_bytes().len();
            }
            Component::CurDir => {}
            Component::ParentDir => {
                // Get the length up to the parent directory
                if let Some(len) = res
                    .parent()
                    .map(|p| p.as_os_str().as_encoded_bytes().len())
                    // Ensure we don't pop the root directory
                    && len >= root_len
                {
                    // Pop the last component from `res`.
                    //
                    // This can be replaced with a plain `res.as_mut_os_string().truncate(len)`
                    // once `os_string_truncate` is stabilized (#133262).
                    let mut bytes = res.into_os_string().into_encoded_bytes();
                    bytes.truncate(len);
                    res = PathBuf::from(unsafe { OsString::from_encoded_bytes_unchecked(bytes) });
                }
            }
            Component::Normal(p) => res.push(p),
        }
    }

    res
}

/// Searches for the project root, which is defined as the directory containing `Cargo.toml`.
/// The search starts from the given path and goes up the directory tree.
pub fn find_project_root(start_path: &Path) -> Option<PathBuf> {
    let mut current_path = start_path.to_path_buf();
    loop {
        if current_path.join("Cargo.toml").is_file() {
            return Some(current_path);
        }
        if !current_path.pop() {
            return None;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::path::Path;
    use tempfile::tempdir;
    use std::fs::File;

    use super::*;

    fn norm(s: &str) -> OsString {
        normalize(Path::new(s)).into_os_string()
    }

    #[cfg(unix)]
    #[test]
    fn test_unix() {
        assert_eq!(norm("/a/b/c"), "/a/b/c");
        assert_eq!(norm("/a/b/c/"), "/a/b/c");
        assert_eq!(norm("/a/./b"), "/a/b");
        assert_eq!(norm("/a/b/../c"), "/a/c");
        assert_eq!(norm("/../../a"), "/a");
        assert_eq!(norm("/../"), "/");
        assert_eq!(norm("/a//b/c"), "/a/b/c");
        assert_eq!(norm("/a/b/c/../../../../d"), "/d");
        assert_eq!(norm("//"), "/");
    }

    #[cfg(windows)]
    #[test]
    fn test_windows() {
        assert_eq!(norm(r"C:\a\b\c"), r"C:\a\b\c");
        assert_eq!(norm(r"C:\a\b\c\"), r"C:\a\b\c");
        assert_eq!(norm(r"C:\a\.\b"), r"C:\a\b");
        assert_eq!(norm(r"C:\a\b\..\c"), r"C:\a\c");
        assert_eq!(norm(r"C:\..\..\a"), r"C:\a");
        assert_eq!(norm(r"C:\..\"), r"C:\");
        assert_eq!(norm(r"C:\a\\b\c"), r"C:\a\b\c");
        assert_eq!(norm(r"C:/a\b/c"), r"C:\a\b\c");
        assert_eq!(norm(r"C:\a\b\c\..\..\..\..\d"), r"C:\d");
        assert_eq!(norm(r"\\server\share\path"), r"\\server\share\path");
    }

    #[test]
    fn test_find_project_root() {
        let dir = tempdir().unwrap();
        let project_root = dir.path();
        let sub_dir = project_root.join("src");
        std::fs::create_dir(&sub_dir).unwrap();
        File::create(project_root.join("Cargo.toml")).unwrap();

        assert_eq!(find_project_root(&sub_dir), Some(project_root.to_path_buf()));
        assert_eq!(find_project_root(project_root), Some(project_root.to_path_buf()));
    }

    #[test]
    fn test_find_project_root_not_found() {
        let dir = tempdir().unwrap();
        let non_project_dir = dir.path();
        let sub_dir = non_project_dir.join("src");
        std::fs::create_dir(&sub_dir).unwrap();

        assert_eq!(find_project_root(&sub_dir), None);
        assert_eq!(find_project_root(non_project_dir), None);
    }
}
