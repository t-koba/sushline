use super::{History, HistoryEntry};
use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

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
        history.import_records(read_history_records(file)?);
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
        history.import_records(read_history_range_records(file, from, to)?);
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
        self.import_records(read_history_records(file)?);
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
        self.import_records(read_history_range_records(file, from, to)?);
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
        write_atomic(path, |file| self.write_entries(file, write_timestamps))
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
        let existed = fs::metadata(path).is_ok();
        let mut file = OpenOptions::new().create(true).append(true).open(path)?;
        #[cfg(unix)]
        if !existed {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
        }
        for entry in self.entries.iter().skip(from) {
            write_entry(&mut file, entry, write_timestamps)?;
        }
        file.sync_all()
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
        let history = Self::read_file(path)?;
        let keep_from = history.entries.len().saturating_sub(max_len);
        write_atomic(path, |file| {
            for entry in &history.entries[keep_from..] {
                write_entry(file, entry, true)?;
            }
            Ok(())
        })
    }

    fn write_entries(&self, file: &mut fs::File, write_timestamps: bool) -> io::Result<()> {
        for entry in &self.entries {
            write_entry(file, entry, write_timestamps)?;
        }
        Ok(())
    }

    fn import_records(&mut self, records: Vec<(Vec<u8>, Option<String>)>) {
        for (line, timestamp) in records {
            self.push_entry(line, timestamp, false);
        }
    }
}

fn read_history_range_records(
    file: fs::File,
    from: usize,
    to: Option<usize>,
) -> io::Result<Vec<(Vec<u8>, Option<String>)>> {
    let count = range_count(from, to);
    Ok(read_history_records(file)?
        .into_iter()
        .skip(from)
        .take(count)
        .collect())
}

fn range_count(from: usize, to: Option<usize>) -> usize {
    match to {
        None => usize::MAX,
        Some(to) if to < from => usize::MAX,
        Some(to) => to.saturating_sub(from).max(1),
    }
}

fn write_atomic(
    path: &Path,
    write_tmp: impl FnOnce(&mut fs::File) -> io::Result<()>,
) -> io::Result<()> {
    let dest = effective_write_path(path);
    let base = history_tmp_path(&dest);
    #[cfg(unix)]
    let target_mode: u32 = {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(&dest)
            .ok()
            .map(|metadata| metadata.permissions().mode() & 0o777)
            .unwrap_or(0o600)
    };
    #[cfg(not(unix))]
    let existing_permissions = fs::metadata(&dest)
        .ok()
        .map(|metadata| metadata.permissions());
    for _ in 0..100 {
        let nonce = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let tmp = unique_tmp_path(&base, nonce);
        let file = match OpenOptions::new().write(true).create_new(true).open(&tmp) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        };
        let result = (|| {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&tmp, fs::Permissions::from_mode(target_mode))?;
            }
            #[cfg(not(unix))]
            if let Some(ref permissions) = existing_permissions {
                fs::set_permissions(&tmp, permissions.clone())?;
            }
            let mut file = file;
            write_tmp(&mut file)?;
            file.sync_all()?;
            fs::rename(&tmp, &dest)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        return result;
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not create unique history tmp file",
    ))
}

fn effective_write_path(path: &Path) -> PathBuf {
    let is_link = fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false);
    if !is_link {
        return path.to_path_buf();
    }
    let Ok(target) = fs::read_link(path) else {
        return path.to_path_buf();
    };
    if target.is_absolute() {
        return target;
    }
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.join(target),
        _ => target,
    }
}

fn unique_tmp_path(base: &Path, nonce: u64) -> PathBuf {
    let mut name = base.as_os_str().to_owned();
    name.push(format!(".{}-{nonce}", std::process::id()));
    PathBuf::from(name)
}

fn is_timestamp_record(line: &str) -> bool {
    let bytes = line.as_bytes();
    bytes.len() >= 2 && bytes[0] == b'#' && bytes[1].is_ascii_digit()
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
    let mut pending_timestamp: Option<String> = None;
    let mut delimited_lines: Vec<Vec<u8>> = Vec::new();
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
            if !delimited_lines.is_empty() {
                let joined = delimited_lines.join(&b'\n');
                delimited_lines.clear();
                records.push((joined, pending_timestamp.take()));
            }
            pending_timestamp = Some(text.to_string());
            line.clear();
            continue;
        }
        if pending_timestamp.is_some() || !delimited_lines.is_empty() {
            // Timestamp-delimited entry: physical lines up to the next
            // timestamp belong to one entry; blank lines are preserved.
            delimited_lines.push(std::mem::take(&mut line));
        } else {
            // Plain file without timestamps: one physical line per entry;
            // blank lines carry no entry.
            if !line.is_empty() {
                records.push((std::mem::take(&mut line), None));
            }
        }
        line.clear();
    }
    if !delimited_lines.is_empty() {
        let joined = delimited_lines.join(&b'\n');
        records.push((joined, pending_timestamp.take()));
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
