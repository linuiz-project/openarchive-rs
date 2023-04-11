#![cfg_attr(not(test), no_std)]
#![feature(
    error_in_core,
    array_chunks                    // #74985 <https://github.com/rust-lang/rust/issues/74985>
)]

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

unsafe impl bytemuck::CheckedBitPattern for ArchiveHeader {
    type Bits = [u8; size_of::<Self>()];

    fn is_valid_bit_pattern(bits: &Self::Bits) -> bool {
        bits.starts_with(Archive::MAGIC)
    }
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

    pub fn entry_data(&self) -> Result<&[u8]> {
        let ptr = NonNull::new(self.end_ptr().cast_mut()).ok_or(Error::InternalError)?;
        Ok(unsafe {
            core::slice::from_raw_parts(
                ptr.as_ptr(),
                (self.entry_count as usize) * size_of::<Self>(),
            )
        })
    }
}

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableEntrySignature {
    File = 0,
    Directory = 1,

    OS(u32) = u32::MAX,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ArchiveTableEntry {
    signature: TableEntrySignature,
    offset: u64,
    length: u64,
    name_len: u64,
    extra_data_len: u64,
}

unsafe impl bytemuck::NoUninit for ArchiveTableEntry {}

unsafe impl bytemuck::CheckedBitPattern for ArchiveTableEntry {
    type Bits = [u8; size_of::<Self>()];

    fn is_valid_bit_pattern(bits: &Self::Bits) -> bool {
        let signature_bits =
            u64::from_le_bytes(bits[..size_of::<TableEntrySignature>()].try_into().unwrap());
        matches!(signature_bits, 0 | 1 | 0xFFFF_FFFF)
    }
}

impl From<ArchiveTableEntry> for [u8; size_of::<ArchiveTableEntry>()] {
    fn from(entry: ArchiveTableEntry) -> Self {
        bytemuck::bytes_of(&entry).try_into().unwrap()
    }
}

pub struct Archive<'a> {
    header: &'a ArchiveHeader,
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
    entry_data: core::slice::ArrayChunks<'a, u8, { size_of::<ArchiveTableEntry>() }>,
    index: usize,
}

impl<'a> Iterator for ArchiveIterator<'a> {
    type Item = Result<&'a ArchiveTableEntry>;

    fn next(&mut self) -> Option<Self::Item> {
        let entry_datum = self.entry_data.next()?;
        self.index += 1;

        Some(bytemuck::checked::try_from_bytes(entry_datum).map_err(|_| Error::InvalidSignature))
    }
}
