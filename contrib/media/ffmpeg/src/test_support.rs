#[cfg(test)]
pub(crate) use inner::write_executable_script;

#[cfg(test)]
mod inner {
    use std::io::Write as _;
    use std::os::unix::fs::PermissionsExt as _;
    use std::path::{Path, PathBuf};

    /// A temporary executable standing in for a real `ffmpeg`/`ffprobe` binary.
    ///
    /// Owns the backing temporary directory so the executable stays on disk for
    /// the lifetime of the handle; named for the boundary role it fills rather
    /// than its shell-script implementation.
    pub(crate) struct TestExecutable {
        _dir: rskit_storage::TempDir,
        path: PathBuf,
    }

    impl TestExecutable {
        pub(crate) fn path(&self) -> &Path {
            &self.path
        }
    }

    pub(crate) fn write_executable_script(body: &str) -> TestExecutable {
        let dir = rskit_storage::TempDir::new().unwrap();
        let path = dir.path().join("script.sh");
        {
            let mut file = std::fs::File::create(&path).unwrap();
            writeln!(file, "#!/bin/sh").unwrap();
            writeln!(file, "{body}").unwrap();
            file.sync_all().unwrap();
        }
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).unwrap();
        TestExecutable { _dir: dir, path }
    }
}
