use super::{History, HistoryEntry};
use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, Write};
use std::path::Path;

impl History {
    pub fn read_file(path: impl AsRef<Path>) -> io::Result<Self> {
        let file = fs::File::open(path)?;
        let mut history = Self::new();
        for (line, timestamp) in read_history_records(file)? {
            history.push_entry(line, timestamp, false);
        }
        history.file_loaded_len = history.entries.len();
        Ok(history)
    }

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

    pub fn write_file(&self, path: impl AsRef<Path>) -> io::Result<()> {
        let path = path.as_ref();
        with_history_lock(path, || {
            let tmp = history_tmp_path(path);
            let mut file = fs::File::create(&tmp)?;
            self.write_entries(&mut file)
                .and_then(|()| file.sync_all())
                .and_then(|()| fs::rename(&tmp, path))
        })
    }

    pub fn append_file(&self, path: impl AsRef<Path>, from: usize) -> io::Result<()> {
        let path = path.as_ref();
        with_history_lock(path, || {
            let mut file = OpenOptions::new().create(true).append(true).open(path)?;
            for entry in self.entries.iter().skip(from) {
                write_entry(&mut file, entry)?;
            }
            file.sync_all()
        })
    }

    pub fn append_new_to_file(&mut self, path: impl AsRef<Path>) -> io::Result<()> {
        self.append_file(path, self.file_loaded_len)?;
        self.file_loaded_len = self.entries.len();
        Ok(())
    }

    pub fn truncate_file(path: impl AsRef<Path>, max_len: usize) -> io::Result<()> {
        let path = path.as_ref();
        with_history_lock(path, || {
            let history = Self::read_file(path)?;
            let keep_from = history.entries.len().saturating_sub(max_len);
            let tmp = history_tmp_path(path);
            let mut file = fs::File::create(&tmp)?;
            for entry in &history.entries[keep_from..] {
                write_entry(&mut file, entry)?;
            }
            file.sync_all()?;
            fs::rename(&tmp, path)
        })
    }

    fn write_entries(&self, file: &mut fs::File) -> io::Result<()> {
        for entry in &self.entries {
            write_entry(file, entry)?;
        }
        Ok(())
    }
}

fn is_timestamp_record(line: &str) -> bool {
    line.strip_prefix('#')
        .is_some_and(|rest| !rest.is_empty() && rest.bytes().all(|byte| byte.is_ascii_digit()))
}

fn write_entry(file: &mut fs::File, entry: &HistoryEntry) -> io::Result<()> {
    if let Some(timestamp) = &entry.timestamp {
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
    Ok(())
}

#[cfg(not(unix))]
fn unlock_file(_file: &fs::File) -> io::Result<()> {
    Ok(())
}
