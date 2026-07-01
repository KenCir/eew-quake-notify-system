use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use thiserror::Error;

use crate::config::RuntimeConfig;

#[derive(Debug)]
pub struct InstanceLock {
    path: PathBuf,
    _file: File,
}

#[derive(Debug, Error)]
pub enum InstanceLockError {
    #[error("another eew-quake-notify-system process appears to be running: {path}")]
    AlreadyRunning { path: PathBuf },
    #[error("failed to create lock directory {path}: {source}")]
    CreateDirectory {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to create lock file {path}: {source}")]
    CreateLock {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to write lock file {path}: {source}")]
    WriteLock {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

impl InstanceLock {
    pub fn acquire(config: &RuntimeConfig) -> Result<Option<Self>, InstanceLockError> {
        if !config.single_instance {
            return Ok(None);
        }

        Self::acquire_path(config.lock_file_path.trim()).map(Some)
    }

    fn acquire_path(path: impl AsRef<Path>) -> Result<Self, InstanceLockError> {
        let path = path.as_ref().to_owned();
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent).map_err(|source| InstanceLockError::CreateDirectory {
                path: parent.to_owned(),
                source,
            })?;
        }

        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|source| {
                if source.kind() == std::io::ErrorKind::AlreadyExists {
                    InstanceLockError::AlreadyRunning { path: path.clone() }
                } else {
                    InstanceLockError::CreateLock {
                        path: path.clone(),
                        source,
                    }
                }
            })?;

        writeln!(file, "pid={}", std::process::id()).map_err(|source| {
            InstanceLockError::WriteLock {
                path: path.clone(),
                source,
            }
        })?;

        Ok(Self { path, _file: file })
    }
}

impl Drop for InstanceLock {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_file(&self.path) {
            tracing::debug!(
                path = %self.path.display(),
                %error,
                "failed to remove instance lock file"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lock_path(name: &str) -> PathBuf {
        std::env::temp_dir()
            .join("eew-quake-notify-system-tests")
            .join(format!("{name}-{}.lock", std::process::id()))
    }

    #[test]
    fn acquires_lock_and_creates_parent_directory() {
        let path = lock_path("acquires-lock");
        let _ = fs::remove_file(&path);

        {
            let lock = InstanceLock::acquire_path(&path).expect("lock should be acquired");
            assert!(path.exists());
            drop(lock);
        }

        assert!(!path.exists());
    }

    #[test]
    fn rejects_second_lock_for_same_file() {
        let path = lock_path("rejects-second-lock");
        let _ = fs::remove_file(&path);
        let _lock = InstanceLock::acquire_path(&path).expect("first lock should be acquired");

        let error = InstanceLock::acquire_path(&path).expect_err("second lock should fail");

        assert!(matches!(error, InstanceLockError::AlreadyRunning { .. }));
    }
}
