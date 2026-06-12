use super::{History, HistoryEntry};
use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

impl History {
    /// Default file path.
    pub fn default_file_path() -> PathBuf {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".history")
    }

    /// Read default file.
    pub fn read_default_file() -> io::Result<Self> {
        Self::read_file(Self::default_file_path())
    }

    /// Read file.
    pub fn read_file(path: impl AsRef<Path>) -> io::Result<Self> {
        let file = fs::File::open(path)?;
        let mut history = Self::new();
        for (line, timestamp) in read_history_records(file)? {
            history.push_entry(line, timestamp, false);
        }
        history.file_loaded_len = history.entries.len();
        Ok(history)
    }

    /// Read file range.
    pub fn read_file_range(
        path: impl AsRef<Path>,
        from: usize,
        to: Option<usize>,
    ) -> io::Result<Self> {
        let file = fs::File::open(path)?;
        let mut history = Self::new();
        let count = range_count(from, to);
        for (line, timestamp) in read_history_records(file)?
            .into_iter()
            .skip(from)
            .take(count)
        {
            history.push_entry(line, timestamp, false);
        }
        history.file_loaded_len = history.entries.len();
        Ok(history)
    }

    /// Load default file.
    pub fn load_default_file(&mut self, max_entries: Option<usize>) -> io::Result<()> {
        self.load_file(Self::default_file_path(), max_entries)
    }

    /// Load file.
    pub fn load_file(
        &mut self,
        path: impl AsRef<Path>,
        max_entries: Option<usize>,
    ) -> io::Result<()> {
        let file = fs::File::open(path)?;
        for (line, timestamp) in read_history_records(file)? {
            self.push_entry(line, timestamp, false);
        }
        self.enforce_max_len(max_entries);
        self.file_loaded_len = self.entries.len();
        Ok(())
    }

    /// Load file range.
    pub fn load_file_range(
        &mut self,
        path: impl AsRef<Path>,
        from: usize,
        to: Option<usize>,
        max_entries: Option<usize>,
    ) -> io::Result<()> {
        let file = fs::File::open(path)?;
        let count = range_count(from, to);
        for (line, timestamp) in read_history_records(file)?
            .into_iter()
            .skip(from)
            .take(count)
        {
            self.push_entry(line, timestamp, false);
        }
        self.enforce_max_len(max_entries);
        self.file_loaded_len = self.entries.len();
        Ok(())
    }

    /// Write default file.
    pub fn write_default_file(&self) -> io::Result<()> {
        self.write_file(Self::default_file_path())
    }

    /// Write file.
    pub fn write_file(&self, path: impl AsRef<Path>) -> io::Result<()> {
        self.write_file_with_timestamps(path, false)
    }

    /// Write file with timestamps.
    pub fn write_file_with_timestamps(
        &self,
        path: impl AsRef<Path>,
        write_timestamps: bool,
    ) -> io::Result<()> {
        let path = path.as_ref();
        with_history_lock(path, || {
            let tmp = history_tmp_path(path);
            let mut file = fs::File::create(&tmp)?;
            self.write_entries(&mut file, write_timestamps)
                .and_then(|()| file.sync_all())
                .and_then(|()| fs::rename(&tmp, path))
        })
    }

    /// Append default file.
    pub fn append_default_file(&self, from: usize) -> io::Result<()> {
        self.append_file(Self::default_file_path(), from)
    }

    /// Append file.
    pub fn append_file(&self, path: impl AsRef<Path>, from: usize) -> io::Result<()> {
        self.append_file_with_timestamps(path, from, false)
    }

    /// Append last to file.
    pub fn append_last_to_file(&self, path: impl AsRef<Path>, nelements: usize) -> io::Result<()> {
        let from = self.entries.len().saturating_sub(nelements);
        self.append_file(path, from)
    }

    /// Append file with timestamps.
    pub fn append_file_with_timestamps(
        &self,
        path: impl AsRef<Path>,
        from: usize,
        write_timestamps: bool,
    ) -> io::Result<()> {
        let path = path.as_ref();
        with_history_lock(path, || {
            let mut file = OpenOptions::new().create(true).append(true).open(path)?;
            for entry in self.entries.iter().skip(from) {
                write_entry(&mut file, entry, write_timestamps)?;
            }
            file.sync_all()
        })
    }

    /// Append new to default file.
    pub fn append_new_to_default_file(&mut self) -> io::Result<()> {
        self.append_new_to_file(Self::default_file_path())
    }

    /// Append new to default file with timestamps.
    pub fn append_new_to_default_file_with_timestamps(
        &mut self,
        write_timestamps: bool,
    ) -> io::Result<()> {
        self.append_new_to_file_with_timestamps(Self::default_file_path(), write_timestamps)
    }

    /// Append new to file.
    pub fn append_new_to_file(&mut self, path: impl AsRef<Path>) -> io::Result<()> {
        self.append_new_to_file_with_timestamps(path, false)
    }

    /// Append new to file with timestamps.
    pub fn append_new_to_file_with_timestamps(
        &mut self,
        path: impl AsRef<Path>,
        write_timestamps: bool,
    ) -> io::Result<()> {
        self.append_file_with_timestamps(path, self.file_loaded_len, write_timestamps)?;
        self.file_loaded_len = self.entries.len();
        Ok(())
    }

    /// Truncate file.
    pub fn truncate_file(path: impl AsRef<Path>, max_len: usize) -> io::Result<()> {
        let path = path.as_ref();
        with_history_lock(path, || {
            let history = Self::read_file(path)?;
            let keep_from = history.entries.len().saturating_sub(max_len);
            let tmp = history_tmp_path(path);
            let mut file = fs::File::create(&tmp)?;
            for entry in &history.entries[keep_from..] {
                write_entry(&mut file, entry, false)?;
            }
            file.sync_all()?;
            fs::rename(&tmp, path)
        })
    }

    fn write_entries(&self, file: &mut fs::File, write_timestamps: bool) -> io::Result<()> {
        for entry in &self.entries {
            write_entry(file, entry, write_timestamps)?;
        }
        Ok(())
    }
}

fn range_count(from: usize, to: Option<usize>) -> usize {
    match to {
        None => usize::MAX,
        Some(to) if to < from => usize::MAX,
        Some(to) => to.saturating_sub(from).max(1),
    }
}

fn is_timestamp_record(line: &str) -> bool {
    line.strip_prefix('#')
        .is_some_and(|rest| !rest.is_empty() && rest.bytes().all(|byte| byte.is_ascii_digit()))
}

fn write_entry(
    file: &mut fs::File,
    entry: &HistoryEntry,
    write_timestamps: bool,
) -> io::Result<()> {
    if write_timestamps && let Some(timestamp) = &entry.timestamp {
        writeln!(file, "{timestamp}")?;
    }
    file.write_all(&entry.line_bytes)?;
    file.write_all(b"\n")
}

fn read_history_records(file: fs::File) -> io::Result<Vec<(Vec<u8>, Option<String>)>> {
    let mut records = Vec::new();
    let mut pending_timestamp = None;
    let mut reader = io::BufReader::new(file);
    let mut line = Vec::new();
    while reader.read_until(b'\n', &mut line)? != 0 {
        if line.ends_with(b"\n") {
            line.pop();
            if line.ends_with(b"\r") {
                line.pop();
            }
        }
        if let Ok(text) = std::str::from_utf8(&line)
            && is_timestamp_record(text)
        {
            pending_timestamp = Some(text.to_string());
            line.clear();
            continue;
        }
        records.push((std::mem::take(&mut line), pending_timestamp.take()));
        line.clear();
    }
    // Ok.
    Ok(records)
}

fn history_tmp_path(path: &Path) -> std::path::PathBuf {
    path.with_extension(format!(
        "{}tmp",
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| format!("{ext}."))
            .unwrap_or_default()
    ))
}

fn history_lock_path(path: &Path) -> std::path::PathBuf {
    path.with_extension(format!(
        "{}lock",
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| format!("{ext}."))
            .unwrap_or_default()
    ))
}

fn with_history_lock<R>(path: &Path, op: impl FnOnce() -> io::Result<R>) -> io::Result<R> {
    let lock_path = history_lock_path(path);
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(lock_path)?;
    lock_file(&lock)?;
    let result = op();
    let unlock_result = unlock_file(&lock);
    match (result, unlock_result) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(err), _) => Err(err),
        (Ok(_), Err(err)) => Err(err),
    }
}

#[cfg(unix)]
fn lock_file(file: &fs::File) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) };
    (rc == 0).then_some(()).ok_or_else(io::Error::last_os_error)
}

#[cfg(unix)]
fn unlock_file(file: &fs::File) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_UN) };
    (rc == 0).then_some(()).ok_or_else(io::Error::last_os_error)
}

#[cfg(not(unix))]
fn lock_file(_file: &fs::File) -> io::Result<()> {
    // Ok.
    Ok(())
}

#[cfg(not(unix))]
fn unlock_file(_file: &fs::File) -> io::Result<()> {
    // Ok.
    Ok(())
}
