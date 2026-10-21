//! The Readline the oracle host carries.
//!
//! The oracle cases compare Sushline against the line editor the host
//! carries, so the compatibility document has to name the Readline the
//! evidence was measured on. The host links Readline either statically, in
//! which case the release number is part of the host's own image, or
//! dynamically, in which case it is part of the shared library the host
//! loads. This module reads the release number out of whichever ELF image
//! carries it, so the version travels with the evidence.
//!
//! A program that links Readline statically keeps the release number in its
//! own symbol table, and it holds the number in the dynamic symbol table
//! only when the program was linked to export its symbols, so both tables
//! are read. A symbol the image does not define is skipped, because a
//! program that loads Readline names the variable without defining it, and
//! the number is in the library the program loads.

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

impl ReadlineVersion {
    /// The release a Readline release number names.
    ///
    /// The number is the release the Readline header declares, so its high
    /// byte is the major release and its low byte is the minor one.
    fn from_release_number(number: u32) -> ReadlineVersion {
        ReadlineVersion {
            major: (number >> 8) & 0xff,
            minor: number & 0xff,
        }
    }
}

/// The Readline releases the compatibility document's evidence covers.
///
/// The reference host carries Readline 8.3, the release the document
/// targets, and the CI-built host is built against that release as well.
/// The 8.2 release is covered too, because the claim about the library's
/// output stream was measured on it. A host that carries another release is
/// not a host the document describes, so the oracle suite stops on it
/// instead of reporting evidence against it.
const DOCUMENTED_VERSIONS: [ReadlineVersion; 2] = [
    ReadlineVersion { major: 8, minor: 2 },
    ReadlineVersion { major: 8, minor: 3 },
];

/// Whether the document's evidence covers a Readline release.
pub(super) fn is_documented(version: ReadlineVersion) -> bool {
    DOCUMENTED_VERSIONS.contains(&version)
}

/// The Readline release the host's rendering evidence was measured on.
///
/// The reference host carries Readline 8.3, the release the document
/// targets, and the pending argument indicator it draws takes the place of
/// the prompt, while the 8.2 release leaves the prompt on the screen, so a
/// case that compares that rendering states the release it needs.
pub(super) const MEASURED_RELEASE: ReadlineVersion = ReadlineVersion { major: 8, minor: 3 };

/// Whether the host carries a given Readline release.
///
/// A case that compares a release-specific behavior cannot report evidence
/// against another release, so it asks for the release first.
pub(super) fn host_carries(version: ReadlineVersion) -> bool {
    host_readline_version() == Some(version)
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
    Some(ReadlineVersion::from_release_number(number))
}

/// A 64-bit little-endian ELF image.
struct Elf<'a> {
    bytes: &'a [u8],
}

/// The section header type of the static symbol table.
const STATIC_SYMBOLS: u32 = 2;

/// The section index a symbol the image does not define carries.
const UNDEFINED_SECTION: u16 = 0;

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

    /// The address of a symbol the image carries.
    ///
    /// The dynamic table holds the symbols the image exports, and the static
    /// table holds the symbols the image was built with, so a program that
    /// links Readline statically and exports no symbols of its own is read
    /// from the static table.
    fn symbol_address(&self, name: &str) -> Option<u64> {
        self.symbol_address_in(DYNAMIC_SYMBOLS, name)
            .or_else(|| self.symbol_address_in(STATIC_SYMBOLS, name))
    }

    /// The address of a symbol in one of the image's symbol tables.
    fn symbol_address_in(&self, kind: u32, name: &str) -> Option<u64> {
        let symbols = self.symbol_table(kind)?;
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
                // A symbol the image does not define carries no value of its
                // own, so it is not the release number this image holds. A
                // host that loads Readline names the variable in its own
                // tables without defining it, and the library the host loads
                // is where the release number lives.
                if self.u16_at(offset + 6)? != UNDEFINED_SECTION {
                    return self.u64_at(offset + 8);
                }
            }
            offset += entry;
        }
        None
    }

    /// The symbol table of a kind the image carries.
    fn symbol_table(&self, kind: u32) -> Option<Section> {
        let count = self.u16_at(0x3c)? as usize;
        (0..count)
            .filter_map(|index| self.section(index))
            .find(|section| section.kind == kind)
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
    use super::{DYNAMIC_SYMBOLS, Elf, ReadlineVersion, STATIC_SYMBOLS, is_documented};

    /// The address the release number is stored at in a test image.
    const NUMBER_ADDRESS: u64 = 0x1000;

    /// Stores a number in a little-endian image.
    fn put_u16(image: &mut [u8], offset: usize, value: u16) {
        image[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }

    /// Stores a number in a little-endian image.
    fn put_u32(image: &mut [u8], offset: usize, value: u32) {
        image[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    /// Stores a number in a little-endian image.
    fn put_u64(image: &mut [u8], offset: usize, value: u64) {
        image[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
    }

    /// Builds an ELF image that carries a release number in a symbol table.
    ///
    /// The image has the shape the reader expects: an ELF header, a program
    /// header that maps the number's address to its file offset, a symbol
    /// table with one entry, and the string table the entry names.
    fn elf_image_with_release_number(kind: u32, number: u32) -> Vec<u8> {
        elf_image_with_symbol(kind, number, 1)
    }

    /// Builds an ELF image whose symbol table names the release number.
    ///
    /// The section the symbol belongs to says whether the image defines the
    /// symbol, so an image that only names it can be built as well.
    fn elf_image_with_symbol(kind: u32, number: u32, section: u16) -> Vec<u8> {
        const DATA_OFFSET: usize = 0x1000;
        const STRING_OFFSET: usize = 0x2000;
        const SYMBOL_OFFSET: usize = 0x2100;
        const HEADER_OFFSET: usize = 0x2200;
        const SECTION_SIZE: usize = 64;

        let mut image = vec![0u8; HEADER_OFFSET + 3 * SECTION_SIZE];

        // The ELF header.
        image[0..4].copy_from_slice(b"\x7fELF");
        image[4] = 2; // A 64-bit image.
        image[5] = 1; // Little-endian bytes.
        image[6] = 1; // The current version.
        put_u16(&mut image, 16, 2); // An executable program.
        put_u16(&mut image, 18, 0x3e); // The x86-64 machine.
        put_u32(&mut image, 20, 1); // The current version.
        put_u64(&mut image, 32, 64); // The program header table.
        put_u64(&mut image, 40, HEADER_OFFSET as u64); // The section header table.
        put_u16(&mut image, 52, 64); // The ELF header's own size.
        put_u16(&mut image, 54, 56); // A program header's own size.
        put_u16(&mut image, 56, 1); // One program header.
        put_u16(&mut image, 58, SECTION_SIZE as u16); // A section header's own size.
        put_u16(&mut image, 60, 3); // Three section headers.
        put_u16(&mut image, 62, 0); // No section header names.

        // The program header that maps the number's address to its bytes.
        put_u32(&mut image, 64, 1); // A loadable segment.
        put_u32(&mut image, 68, 6);
        put_u64(&mut image, 72, DATA_OFFSET as u64);
        put_u64(&mut image, 80, DATA_OFFSET as u64);
        put_u64(&mut image, 88, DATA_OFFSET as u64);
        put_u64(&mut image, 96, 4); // The segment's size in the file.
        put_u64(&mut image, 104, 4);
        put_u64(&mut image, 112, 1);

        // The release number the symbol names.
        put_u32(&mut image, DATA_OFFSET, number);

        // The string table and the symbol that names the number.
        let name = b"rl_readline_version";
        image[STRING_OFFSET] = 0;
        image[STRING_OFFSET + 1..STRING_OFFSET + 1 + name.len()].copy_from_slice(name);
        put_u32(&mut image, SYMBOL_OFFSET, 1); // The name's offset.
        image[SYMBOL_OFFSET + 4] = 0x11; // A global object.
        put_u16(&mut image, SYMBOL_OFFSET + 6, section); // The section it belongs to.
        put_u64(&mut image, SYMBOL_OFFSET + 8, DATA_OFFSET as u64);
        put_u64(&mut image, SYMBOL_OFFSET + 16, 4);

        // The section headers: the symbol table, the string table, and the
        // null header every table starts with.
        let symbols = HEADER_OFFSET + SECTION_SIZE;
        put_u32(&mut image, symbols + 4, kind);
        put_u64(&mut image, symbols + 24, SYMBOL_OFFSET as u64);
        put_u64(&mut image, symbols + 32, 24); // One symbol.
        put_u32(&mut image, symbols + 40, 2); // The string table's section.
        put_u64(&mut image, symbols + 56, 24); // A symbol's own size.
        let strings = HEADER_OFFSET + 2 * SECTION_SIZE;
        put_u32(&mut image, strings + 4, 3); // The string table.
        put_u64(&mut image, strings + 24, STRING_OFFSET as u64);
        put_u64(&mut image, strings + 32, (1 + name.len() + 1) as u64);

        image
    }

    #[test]
    fn the_release_number_is_read_from_the_static_symbol_table() {
        // A host that links Readline statically keeps the release number in
        // its own symbol table, and it exports no symbols of its own unless
        // it was linked to export them.
        let image = elf_image_with_release_number(STATIC_SYMBOLS, 0x0803);
        let elf = Elf::parse(&image).expect("the image is an ELF image");
        let address = elf
            .symbol_address("rl_readline_version")
            .expect("the static symbol table must be read");
        assert_eq!(address, NUMBER_ADDRESS);
        assert_eq!(elf.i32_at(address), Some(0x0803));
    }

    #[test]
    fn the_release_number_is_read_from_the_dynamic_symbol_table() {
        let image = elf_image_with_release_number(DYNAMIC_SYMBOLS, 0x0802);
        let elf = Elf::parse(&image).expect("the image is an ELF image");
        let address = elf
            .symbol_address("rl_readline_version")
            .expect("the dynamic symbol table must be read");
        assert_eq!(address, NUMBER_ADDRESS);
        assert_eq!(elf.i32_at(address), Some(0x0802));
    }

    #[test]
    fn a_symbol_the_image_does_not_define_is_not_read() {
        // A host that loads Readline names the release number in its own
        // tables without defining it, and the number is in the library the
        // host loads, so the host's own image does not carry it.
        let image = elf_image_with_symbol(DYNAMIC_SYMBOLS, 0x0803, 0);
        let elf = Elf::parse(&image).expect("the image is an ELF image");
        assert_eq!(elf.symbol_address("rl_readline_version"), None);
    }

    #[test]
    fn a_release_number_names_its_release() {
        assert_eq!(
            ReadlineVersion::from_release_number(0x0803),
            ReadlineVersion { major: 8, minor: 3 }
        );
        assert_eq!(
            ReadlineVersion::from_release_number(0x0802),
            ReadlineVersion { major: 8, minor: 2 }
        );
    }

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
