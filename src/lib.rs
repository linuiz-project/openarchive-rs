#![cfg_attr(not(test), no_std)]
#![feature(error_in_core)]

use core::{mem::size_of, ptr::NonNull};

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    InvalidMagic,
    InvalidVersion,
    InvalidSignature,
    InternalError,
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Debug::fmt(self, f)
    }
}

impl core::error::Error for Error {}

pub type Result<T> = core::result::Result<T, Error>;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ArchiveHeader {
    _magic: [u8; Archive::MAGIC.len()],
    version: u32,
    entry_count: u32,
    uncompressed_size: u64,
}

impl ArchiveHeader {
    #[inline]
    pub const fn version(&self) -> u32 {
        self.version
    }

    #[inline]
    pub const fn entry_count(&self) -> u32 {
        self.entry_count
    }

    #[inline]
    pub const fn uncompressed_size(&self) -> u64 {
        self.uncompressed_size
    }

    const fn end_ptr(&self) -> *const u8 {
        (self as *const Self as *const u8).wrapping_add(size_of::<Self>())
    }

    pub fn entry_table(&self) -> Result<NonNull<[ArchiveTableEntry]>> {
        let ptr =
            NonNull::new(self.end_ptr() as *const ArchiveTableEntry).ok_or(Error::InternalError)?;
        Ok(unsafe { core::ptr::NonNull::from_raw_parts(ptr, self.entry_count as usize) })
    }
}

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableEntrySignature {
    File = 0,
    Directory = 1,

    OS(u32) = u32::MAX,
}

unsafe impl bytemuck::CheckedBitPattern for TableEntrySignature {
    type Bits = u64;

    fn is_valid_bit_pattern(bits: &Self::Bits) -> bool {
        matches!(bits, 0 | 1 | u32::MAX)
    }
}

#[repr(C)]
pub struct ArchiveTableEntry {
    signature: TableEntrySignature,
    offset: u64,
    length: u64,
    name_len: u64,
    extra_data_len: u64,
}

pub struct Archive<'a> {
    header: &'a ArchiveHeader,
}

unsafe impl bytemuck::CheckedBitPattern for ArchiveHeader {
    type Bits = [u8; size_of::<Self>()];

    fn is_valid_bit_pattern(bits: &Self::Bits) -> bool {
        bits.starts_with(Archive::MAGIC)
    }
}

impl core::ops::Deref for Archive<'_> {
    type Target = ArchiveHeader;

    fn deref(&self) -> &Self::Target {
        self.header
    }
}

impl<'a> Archive<'a> {
    pub const MAGIC: &[u8; 8] = b"OARCHIVE";
    pub const VERSIONS: [u32; 1] = [u32::from_le_bytes([0, 0, 1, 0])];

    pub fn from_bytes(data: &'a [u8]) -> Result<Self> {
        let header = bytemuck::checked::try_from_bytes::<ArchiveHeader>(data)
            .map_err(|_| Error::InvalidMagic)?;

        if !Self::VERSIONS.contains(&header.version) {
            return Err(Error::InvalidVersion);
        }

        Ok(Self { header })
    }
}

pub struct ArchiveIterator<'a> {
    archive: &'a Archive<'a>,
    index: usize,
}

impl Iterator for ArchiveIterator<'_> {
    type Item = ArchiveTableEntry;

    fn next(&mut self) -> Option<Self::Item> {
        let entry = self.archive.entry_table().get(self.index)?;
        self.index += 1;

        Some(ArchiveEntry {
            archive: self.archive,
            entry,
        })
    }
}
