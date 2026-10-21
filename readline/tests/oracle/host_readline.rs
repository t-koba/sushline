//! The Readline the oracle host carries.
//!
//! The oracle cases compare Sushline against the line editor the host
//! carries, so the compatibility document has to name the Readline the
//! evidence was measured on. The host links Readline either statically, in
//! which case the release number is part of the host's own image, or
//! dynamically, in which case it is part of the shared library the host
//! loads. This module reads the release number out of whichever ELF image
//! carries it, so the version travels with the evidence.

use std::path::{Path, PathBuf};
use std::process::Command;

/// A Readline release, as `major.minor`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ReadlineVersion {
    major: u32,
    minor: u32,
}

impl std::fmt::Display for ReadlineVersion {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}.{}", self.major, self.minor)
    }
}

/// The Readline releases the compatibility document's evidence covers.
///
/// The reference host carries Readline 8.3, the release the document
/// targets, and the CI-built host carries the Readline the runner image
/// ships, which was 8.2 when the document was written. A host that carries
/// another release is not a host the document describes, so the oracle suite
/// stops on it instead of reporting evidence against it.
const DOCUMENTED_VERSIONS: [ReadlineVersion; 2] = [
    ReadlineVersion { major: 8, minor: 2 },
    ReadlineVersion { major: 8, minor: 3 },
];

/// Whether the document's evidence covers a Readline release.
pub(super) fn is_documented(version: ReadlineVersion) -> bool {
    DOCUMENTED_VERSIONS.contains(&version)
}

/// The Readline the host on `PATH` carries.
///
/// The measurement needs the host's ELF image, so it reports nothing on a
/// platform whose executables are not ELF images.
pub(super) fn host_readline_version() -> Option<ReadlineVersion> {
    let executable = host_executable()?;
    if let Some(version) = release_number_in(&executable) {
        return Some(version);
    }
    let library = linked_readline_library(&executable)?;
    release_number_in(&library)
}

/// The host program the oracle suite runs.
fn host_executable() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|directory| directory.join("bash"))
        .find(|candidate| candidate.is_file())
}

/// The Readline library the host loads, if it loads one.
///
/// The loader reports the libraries a program maps, so the shared library
/// that carries the release number is found from the loader's own answer.
fn linked_readline_library(executable: &Path) -> Option<PathBuf> {
    let output = Command::new("ldd").arg(executable).output().ok()?;
    library_path(&String::from_utf8_lossy(&output.stdout))
}

/// The Readline library a loader report names.
///
/// The report lists every library a program maps, each as the library's
/// name, the path it was found at, and the address it was mapped to.
fn library_path(report: &str) -> Option<PathBuf> {
    report.lines().find_map(|line| {
        let (_, target) = line.split_once("=>")?;
        let target = target.trim();
        let path = target.split_whitespace().next()?;
        if path.contains("libreadline") && path.starts_with('/') {
            Some(PathBuf::from(path))
        } else {
            None
        }
    })
}

/// The Readline release an ELF image carries.
///
/// The library exports the release number as `rl_readline_version`, the
/// value the Readline header declares for the release, so the number
/// identifies the release the image was built from.
fn release_number_in(image: &Path) -> Option<ReadlineVersion> {
    let bytes = std::fs::read(image).ok()?;
    let elf = Elf::parse(&bytes)?;
    let address = elf.symbol_address("rl_readline_version")?;
    let number = elf.i32_at(address)? as u32;
    Some(ReadlineVersion {
        major: (number >> 8) & 0xff,
        minor: number & 0xff,
    })
}

/// A 64-bit little-endian ELF image.
struct Elf<'a> {
    bytes: &'a [u8],
}

/// The section header type of the dynamic symbol table.
const DYNAMIC_SYMBOLS: u32 = 11;

impl<'a> Elf<'a> {
    /// Reads an image, when it is an ELF image this module understands.
    fn parse(bytes: &'a [u8]) -> Option<Elf<'a>> {
        if bytes.get(0..4)? != b"\x7fELF" {
            return None;
        }
        // Only 64-bit little-endian images are read.
        if bytes.get(4)? != &2 || bytes.get(5)? != &1 {
            return None;
        }
        Some(Elf { bytes })
    }

    fn u16_at(&self, offset: usize) -> Option<u16> {
        let raw = self.bytes.get(offset..offset + 2)?;
        Some(u16::from_le_bytes([raw[0], raw[1]]))
    }

    fn u32_at(&self, offset: usize) -> Option<u32> {
        let raw = self.bytes.get(offset..offset + 4)?;
        Some(u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]))
    }

    fn u64_at(&self, offset: usize) -> Option<u64> {
        let raw = self.bytes.get(offset..offset + 8)?;
        let mut value = [0u8; 8];
        value.copy_from_slice(raw);
        Some(u64::from_le_bytes(value))
    }

    /// The address of a symbol the image exports.
    fn symbol_address(&self, name: &str) -> Option<u64> {
        let count = self.u16_at(0x3c)? as usize;
        let symbols = (0..count)
            .filter_map(|index| self.section(index))
            .find(|section| section.kind == DYNAMIC_SYMBOLS)?;
        let strings = self.section(symbols.link as usize)?;
        let entry = if symbols.entry_size == 0 {
            24
        } else {
            symbols.entry_size
        };
        let mut offset = symbols.offset;
        while offset + 24 <= symbols.offset + symbols.size {
            let name_offset = self.u32_at(offset)? as usize;
            if self.string(strings.offset + name_offset)? == name {
                return self.u64_at(offset + 8);
            }
            offset += entry;
        }
        None
    }

    /// A section header of the image.
    fn section(&self, index: usize) -> Option<Section> {
        let table = self.u64_at(0x28)? as usize;
        let size = self.u16_at(0x3a)? as usize;
        let offset = table + index * size;
        Some(Section {
            kind: self.u32_at(offset + 4)?,
            offset: self.u64_at(offset + 24)? as usize,
            size: self.u64_at(offset + 32)? as usize,
            link: self.u32_at(offset + 40)?,
            entry_size: self.u64_at(offset + 56)? as usize,
        })
    }

    /// The string at an offset of the image.
    fn string(&self, offset: usize) -> Option<&str> {
        let rest = self.bytes.get(offset..)?;
        let end = rest.iter().position(|byte| *byte == 0)?;
        std::str::from_utf8(rest.get(..end)?).ok()
    }

    /// The signed number an address holds.
    fn i32_at(&self, address: u64) -> Option<i32> {
        let offset = self.file_offset(address)?;
        let raw = self.bytes.get(offset..offset + 4)?;
        Some(i32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]))
    }

    /// The file offset an address is stored at.
    fn file_offset(&self, address: u64) -> Option<usize> {
        let table = self.u64_at(0x20)? as usize;
        let size = self.u16_at(0x36)? as usize;
        let count = self.u16_at(0x38)? as usize;
        for index in 0..count {
            let offset = table + index * size;
            if self.u32_at(offset)? != 1 {
                continue;
            }
            let file_offset = self.u64_at(offset + 8)?;
            let virtual_address = self.u64_at(offset + 16)?;
            let file_size = self.u64_at(offset + 32)?;
            if virtual_address <= address && address < virtual_address + file_size {
                return Some((file_offset + (address - virtual_address)) as usize);
            }
        }
        None
    }
}

/// A section header of an ELF image.
struct Section {
    kind: u32,
    offset: usize,
    size: usize,
    link: u32,
    entry_size: usize,
}

#[cfg(test)]
mod tests {
    use super::{ReadlineVersion, is_documented};

    #[test]
    fn the_documented_releases_are_the_ones_the_document_covers() {
        assert!(is_documented(ReadlineVersion { major: 8, minor: 2 }));
        assert!(is_documented(ReadlineVersion { major: 8, minor: 3 }));
        assert!(!is_documented(ReadlineVersion { major: 8, minor: 1 }));
        assert!(!is_documented(ReadlineVersion { major: 8, minor: 4 }));
    }

    #[test]
    fn the_loader_report_names_the_readline_library() {
        let report = "\tlinux-vdso.so.1 (0x00007ffd9c3f9000)\n\tlibreadline.so.8 => /lib/x86_64-linux-gnu/libreadline.so.8 (0x00007f9c2b1c4000)\n\tlibc.so.6 => /lib/x86_64-linux-gnu/libc.so.6 (0x00007f9c2a1ea000)\n";
        assert_eq!(
            super::library_path(report),
            Some(std::path::PathBuf::from(
                "/lib/x86_64-linux-gnu/libreadline.so.8"
            ))
        );
    }

    #[test]
    fn a_loader_report_without_a_readline_library_names_none() {
        let report = "\tlinux-vdso.so.1 (0x00007ffd9c3f9000)\n\tlibc.so.6 => /lib/x86_64-linux-gnu/libc.so.6 (0x00007f9c2a1ea000)\n";
        assert_eq!(super::library_path(report), None);
    }

    #[test]
    fn a_release_is_written_as_its_numbers() {
        assert_eq!(
            ReadlineVersion { major: 8, minor: 3 }.to_string(),
            "8.3".to_string()
        );
    }
}
