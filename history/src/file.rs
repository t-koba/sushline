use super::{History, HistoryEntry, HistoryMultiline};
use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);
const MAX_SYMLINK_HOPS: usize = 40;

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
        Self::read_file_with_multiline(path, HistoryMultiline::default())
    }

    /// Read file with a multiline policy.
    ///
    /// The policy decides how the file's physical lines are grouped into
    /// entries; the default groups one entry per physical line, which is
    /// GNU's default `history_multiline_entries` mode.
    pub fn read_file_with_multiline(
        path: impl AsRef<Path>,
        multiline: HistoryMultiline,
    ) -> io::Result<Self> {
        let file = fs::File::open(path)?;
        let mut history = Self::new();
        history.import_records(read_history_records(file, multiline)?);
        history.file_loaded_len = history.entries.len();
        Ok(history)
    }

    /// Read file range.
    ///
    /// The file is read with the default `HistoryMultiline` grouping. The
    /// FROM/TO bounds count physical command lines and blank lines are not
    /// counted. For a FROM below TO the TO bound is exclusive; equal bounds
    /// select the command line at FROM, and a TO below FROM leaves the range
    /// open to the end of the file.
    pub fn read_file_range(
        path: impl AsRef<Path>,
        from: usize,
        to: Option<usize>,
    ) -> io::Result<Self> {
        Self::read_file_range_with_multiline(path, from, to, HistoryMultiline::default())
    }

    /// Read file range with a multiline policy.
    ///
    /// The FROM/TO bounds count physical command lines and blank lines are
    /// not counted; the selected lines are then grouped by the policy, so a
    /// range can end with a partial block. That is what GNU's own range call
    /// does, as measured against the GNU history library.
    ///
    /// For a FROM below TO the TO bound is exclusive; equal bounds select the
    /// command line at FROM, and a TO below FROM leaves the range open to
    /// the end of the file. Those bounds are this product's own rule: the
    /// measured GNU release returns the command line at FROM for a file
    /// without timestamp records, and nothing for some files with them.
    pub fn read_file_range_with_multiline(
        path: impl AsRef<Path>,
        from: usize,
        to: Option<usize>,
        multiline: HistoryMultiline,
    ) -> io::Result<Self> {
        let file = fs::File::open(path)?;
        let mut history = Self::new();
        history.import_records(read_history_range_records(file, from, to, multiline)?);
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
        self.load_file_with_multiline(path, max_entries, HistoryMultiline::default())
    }

    /// Load file with a multiline policy.
    ///
    /// The policy decides how the file's physical lines are grouped into
    /// entries; the default groups one entry per physical line, which is
    /// GNU's default `history_multiline_entries` mode.
    pub fn load_file_with_multiline(
        &mut self,
        path: impl AsRef<Path>,
        max_entries: Option<usize>,
        multiline: HistoryMultiline,
    ) -> io::Result<()> {
        let file = fs::File::open(path)?;
        self.import_records(read_history_records(file, multiline)?);
        self.enforce_max_len(max_entries);
        self.file_loaded_len = self.entries.len();
        Ok(())
    }

    /// Load file range.
    ///
    /// The file is read with the default `HistoryMultiline` grouping. The
    /// FROM/TO bounds count physical command lines and blank lines are not
    /// counted. For a FROM below TO the TO bound is exclusive; equal bounds
    /// select the command line at FROM, and a TO below FROM leaves the range
    /// open to the end of the file.
    pub fn load_file_range(
        &mut self,
        path: impl AsRef<Path>,
        from: usize,
        to: Option<usize>,
        max_entries: Option<usize>,
    ) -> io::Result<()> {
        self.load_file_range_with_multiline(
            path,
            from,
            to,
            max_entries,
            HistoryMultiline::default(),
        )
    }

    /// Load file range with a multiline policy.
    ///
    /// The FROM/TO bounds count physical command lines and blank lines are
    /// not counted; the selected lines are then grouped by the policy, so a
    /// range can end with a partial block. That is what GNU's own range call
    /// does, as measured against the GNU history library.
    ///
    /// For a FROM below TO the TO bound is exclusive; equal bounds select the
    /// command line at FROM, and a TO below FROM leaves the range open to
    /// the end of the file. Those bounds are this product's own rule: the
    /// measured GNU release returns the command line at FROM for a file
    /// without timestamp records, and nothing for some files with them.
    pub fn load_file_range_with_multiline(
        &mut self,
        path: impl AsRef<Path>,
        from: usize,
        to: Option<usize>,
        max_entries: Option<usize>,
        multiline: HistoryMultiline,
    ) -> io::Result<()> {
        let file = fs::File::open(path)?;
        self.import_records(read_history_range_records(file, from, to, multiline)?);
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
        let mut append_opts = OpenOptions::new();
        append_opts.create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            append_opts.mode(0o600);
        }
        let mut file = append_opts.open(path)?;
        #[cfg(unix)]
        if !existed {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
        }
        for entry in self.entries.iter().skip(from) {
            write_entry(&mut file, entry, write_timestamps)?;
        }
        // Close-only durability matches GNU: no fsync, close surfaces writes.
        Ok(())
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
    ///
    /// The file is read with the default per-physical-line grouping, so the
    /// entries that are kept are the file's command lines. The truncate is a
    /// rewrite of the file from those entries, not an edit of its bytes: the
    /// rewritten file has no blank lines, an unterminated trailing line is
    /// dropped, and when the kept window starts inside a timestamp block the
    /// block's timestamp record is dropped and the kept lines are written
    /// without a timestamp.
    pub fn truncate_file(path: impl AsRef<Path>, max_len: usize) -> io::Result<()> {
        let path = path.as_ref();
        let dest = effective_write_path(path)?;
        if dest_is_non_regular(&dest) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "history truncate requires a regular file",
            ));
        }
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
    multiline: HistoryMultiline,
) -> io::Result<Vec<(Vec<u8>, Option<String>)>> {
    let count = range_count(from, to);
    let blocks = read_history_blocks(file)?;
    Ok(group_history_blocks(
        &blocks,
        multiline,
        Some((from, count)),
    ))
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
    let dest = effective_write_path(path)?;
    if dest_is_non_regular(&dest) {
        return write_in_place(path, write_tmp);
    }
    let base = history_tmp_path(&dest);
    for _ in 0..100 {
        let nonce = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let tmp = unique_tmp_path(&base, nonce);
        let mut tmp_opts = OpenOptions::new();
        tmp_opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            tmp_opts.mode(0o600);
        }
        let file = match tmp_opts.open(&tmp) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        };
        let result = (|| {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600))?;
            }
            let mut file = file;
            write_tmp(&mut file)?;
            // Close-only durability matches GNU: no fsync before rename.
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

fn dest_is_non_regular(dest: &Path) -> bool {
    fs::metadata(dest)
        .map(|metadata| !metadata.file_type().is_file())
        .unwrap_or(false)
}

fn write_in_place(
    path: &Path,
    write_tmp: impl FnOnce(&mut fs::File) -> io::Result<()>,
) -> io::Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)?;
    write_tmp(&mut file)?;
    // Close-only durability matches GNU: no fsync for non-regular destinations.
    Ok(())
}

fn effective_write_path(path: &Path) -> io::Result<PathBuf> {
    let mut current = path.to_path_buf();
    let mut hops = 0;
    loop {
        let metadata = match fs::symlink_metadata(&current) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(current),
            Err(error) => return Err(error),
        };
        if !metadata.file_type().is_symlink() {
            return Ok(current);
        }
        if hops == MAX_SYMLINK_HOPS {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "too many symbolic links in history path",
            ));
        }
        hops += 1;

        let target = fs::read_link(&current)?;
        current = if target.is_absolute() {
            target
        } else {
            current
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."))
                .join(target)
        };
    }
}

fn unique_tmp_path(base: &Path, nonce: u64) -> PathBuf {
    let mut name = base.as_os_str().to_owned();
    name.push(format!(".{}-{nonce}", std::process::id()));
    PathBuf::from(name)
}

fn is_timestamp_record(line: &[u8]) -> bool {
    line.len() >= 2 && line[0] == b'#' && line[1].is_ascii_digit()
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

fn read_history_records(
    file: fs::File,
    multiline: HistoryMultiline,
) -> io::Result<Vec<(Vec<u8>, Option<String>)>> {
    let blocks = read_history_blocks(file)?;
    Ok(group_history_blocks(&blocks, multiline, None))
}

/// The physical lines a timestamp record opens, or the lines that precede
/// the first timestamp record.
struct HistoryBlock {
    /// The timestamp record that opened the block; None for the lines
    /// before the first timestamp record.
    timestamp: Option<String>,
    /// The physical lines of the block.
    lines: Vec<Vec<u8>>,
}

fn read_history_blocks(file: fs::File) -> io::Result<Vec<HistoryBlock>> {
    // Match GNU read_history_range rejecting non-regular descriptors after
    // open (EINVAL): /dev/null and similar specials error instead of loading
    // as empty, while symlinks resolve to their target descriptor.
    if !file.metadata()?.file_type().is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "history read requires a regular file",
        ));
    }
    let mut blocks: Vec<HistoryBlock> = Vec::new();
    let mut reader = io::BufReader::new(file);
    let mut line = Vec::new();
    while reader.read_until(b'\n', &mut line)? != 0 {
        if !line.ends_with(b"\n") {
            // GNU ingests a physical line only when the scan hits LF, so a
            // trailing EOF fragment without a terminator is ignored while
            // preceding completed lines are retained.
            line.clear();
            break;
        }
        line.pop();
        if line.ends_with(b"\r") {
            line.pop();
        }
        // GNU recognizes a timestamp record from its raw `#` plus ASCII
        // digit prefix. Invalid UTF-8 in the remaining metadata must not turn
        // the delimiter into command text; the public String field stores it
        // with replacement characters.
        if is_timestamp_record(&line) {
            // A timestamp record opens the block of the physical lines that
            // follow it.
            blocks.push(HistoryBlock {
                timestamp: Some(String::from_utf8_lossy(&line).into_owned()),
                lines: Vec::new(),
            });
            line.clear();
            continue;
        }
        if blocks.is_empty() {
            // The lines before the first timestamp record carry no
            // timestamp of their own.
            blocks.push(HistoryBlock {
                timestamp: None,
                lines: Vec::new(),
            });
        }
        blocks
            .last_mut()
            .expect("a block was just pushed")
            .lines
            .push(std::mem::take(&mut line));
        line.clear();
    }
    Ok(blocks)
}

/// Group the blocks' physical lines into entries.
///
/// The selection bounds the grouping to a run of physical command lines,
/// the unit GNU's range bounds count; without it every command line of the
/// file is grouped.
fn group_history_blocks(
    blocks: &[HistoryBlock],
    multiline: HistoryMultiline,
    selection: Option<(usize, usize)>,
) -> Vec<(Vec<u8>, Option<String>)> {
    let mut records = Vec::new();
    let mut command_index = 0usize;
    for block in blocks {
        // The command lines of the block: the physical lines that carry an
        // entry. A blank line carries none.
        let command_lines: Vec<usize> = block
            .lines
            .iter()
            .enumerate()
            .filter(|(_, line)| !line.is_empty())
            .map(|(index, _)| index)
            .collect();
        let block_start = command_index;
        command_index += command_lines.len();

        let selected: Vec<usize> = match selection {
            None => command_lines.clone(),
            Some((from, count)) => {
                let end = from.saturating_add(count);
                command_lines
                    .iter()
                    .enumerate()
                    .filter(|(offset, _)| {
                        let index = block_start + offset;
                        index >= from && index < end
                    })
                    .map(|(_, line)| *line)
                    .collect()
            }
        };
        if selected.is_empty() {
            continue;
        }
        // The timestamp record applies to the first command line of its
        // block; a range that starts inside the block selects no timestamp.
        let carrier = command_lines.first().copied();
        let (start, end) = match selection {
            // The whole block, so that a joined entry keeps the blank lines
            // that follow its last command line.
            None => (0, block.lines.len().saturating_sub(1)),
            Some(_) => (
                selected[0],
                *selected.last().expect("the selection is not empty"),
            ),
        };

        if multiline == HistoryMultiline::Joined && block.timestamp.is_some() {
            // The physical lines that follow the timestamp record make one
            // entry; leading blank lines are dropped and interior blanks are
            // preserved.
            let Some(first) = block.lines[start..=end]
                .iter()
                .position(|line| !line.is_empty())
            else {
                continue;
            };
            let joined = block.lines[start + first..=end].join(&b'\n');
            let timestamp = match carrier {
                Some(carrier) if carrier >= start && carrier <= end => block.timestamp.clone(),
                _ => None,
            };
            records.push((joined, timestamp));
        } else {
            // One entry per physical command line.
            for line in selected {
                let timestamp = match carrier {
                    Some(carrier) if carrier == line => block.timestamp.clone(),
                    _ => None,
                };
                records.push((block.lines[line].clone(), timestamp));
            }
        }
    }
    records
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
