#![cfg_attr(not(test), no_std)]
#![feature(
    error_in_core,
    array_chunks                    // #74985 <https://github.com/rust-lang/rust/issues/74985>
)]

#[cfg(feature = "alloc")]
extern crate alloc;

#[cfg(feature = "alloc")]
pub mod builder;

use core::mem::size_of;

pub const MAGIC: &[u8; 8] = b"OARCHIVE";

pub const VERSION_0_0_1_0: u32 = u32::from_le_bytes([0, 0, 1, 0]);

pub const VERSIONS: [u32; 1] = [VERSION_0_0_1_0];

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    InvalidMagic,
    InvalidVersion,
    InvalidSignature,
    InternalError,
    IncompleteHeader,
    InvalidSizeSum,
    IncompleteData,
    InvalidEntryTable,
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Debug::fmt(self, f)
    }
}

impl core::error::Error for Error {}

pub type Result<T> = core::result::Result<T, Error>;

impl<'a> Archive<'a> {
    pub fn new(data: &'a [u8]) -> Result<Self> {
        // TODO pre-handle panic conditions for `split_at`

        let (header_bytes, data) = data.split_at(size_of::<ArchiveHeader>());
        let header = <&ArchiveHeader>::try_from(header_bytes)?;

        let entry_table_size =
            usize::try_from(header.entry_count).unwrap() * size_of::<ArchiveTableEntry>();
        let (entry_table_bytes, data) = data.split_at(entry_table_size);
        let entry_table = bytemuck::checked::try_cast_slice(entry_table_bytes)
            .map_err(|_| Error::InvalidEntryTable)?;

        let (names_bytes, data) = data.split_at(usize::try_from(header.names_size).unwrap());
        let (extra_data_bytes, data) =
            data.split_at(usize::try_from(header.extra_data_size).unwrap());

        Ok(Self {
            header,
            entry_table,
            names: names_bytes,
            extra_data: extra_data_bytes,
            data,
        })
    }

    pub fn iter(&self) -> ArchiveIterator {
        ArchiveIterator {
            entries: self.entry_table,
            names: self.names,
            extra_data: self.extra_data,
            data: self.data,
            index: 0,
        }
    }
}

impl core::fmt::Debug for Archive<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Archive")
            .field("Version", &self.version.to_le_bytes())
            .field("Entry Count", &self.entry_count())
            .field("Names Size", &self.names_size())
            .field("Names Bytes", &self.names)
            .field("Extra Data Size", &self.extra_data_size())
            .field("Extra Data Bytes", &self.extra_data)
            .field("Uncompressed Size", &self.uncompressed_size())
            .field("Data Bytes", &self.data)
            .finish()
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ArchiveHeader {
    _magic: [u8; MAGIC.len()],
    version: u32,
    entry_count: u32,
    names_size: u64,
    extra_data_size: u64,
    uncompressed_size: u64,
}

unsafe impl bytemuck::Zeroable for ArchiveHeader {}
unsafe impl bytemuck::Pod for ArchiveHeader {}

impl<'a> TryFrom<&'a [u8]> for &'a ArchiveHeader {
    type Error = Error;

    fn try_from(bytes: &'a [u8]) -> Result<Self> {
        let header = bytemuck::try_from_bytes::<ArchiveHeader>(bytes)
            .map_err(|_| Error::IncompleteHeader)?;

        if &header._magic != MAGIC {
            Err(Error::InvalidMagic)
        } else if !VERSIONS.contains(&header.version) {
            Err(Error::InvalidVersion)
        } else {
            Ok(header)
        }
    }
}

impl ArchiveHeader {
    pub(crate) fn new(
        version: u32,
        entry_count: u32,
        names_size: u64,
        extra_data_size: u64,
        uncompressed_size: u64,
    ) -> Self {
        ArchiveHeader {
            _magic: *MAGIC,
            version,
            entry_count,
            names_size,
            extra_data_size,
            uncompressed_size,
        }
    }

    #[inline]
    pub const fn version(&self) -> u32 {
        self.version
    }

    #[inline]
    pub const fn entry_count(&self) -> u32 {
        self.entry_count
    }

    #[inline]
    pub const fn names_size(&self) -> u64 {
        self.names_size
    }

    #[inline]
    pub const fn extra_data_size(&self) -> u64 {
        self.extra_data_size
    }

    #[inline]
    pub const fn uncompressed_size(&self) -> u64 {
        self.uncompressed_size
    }
}

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signature {
    File = 0,
    Directory = 1,

    OS(u32) = u32::MAX,
}

unsafe impl bytemuck::CheckedBitPattern for Signature {
    type Bits = [u8; size_of::<Self>()];

    fn is_valid_bit_pattern(bits: &Self::Bits) -> bool {
        matches!(
            u32::from_le_bytes(bits[..size_of::<u32>()].try_into().unwrap()),
            0 | 1 | u32::MAX
        )
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub(crate) struct ArchiveTableEntry {
    signature: Signature,
    name_offset: u64,
    name_len: u64,
    extra_data_offset: u64,
    extra_data_len: u64,
    data_offset: u64,
    data_len: u64,
}

unsafe impl bytemuck::NoUninit for ArchiveTableEntry {}
unsafe impl bytemuck::CheckedBitPattern for ArchiveTableEntry {
    type Bits = [u8; size_of::<Self>()];

    fn is_valid_bit_pattern(bits: &Self::Bits) -> bool {
        <Signature as bytemuck::CheckedBitPattern>::is_valid_bit_pattern(
            bits[..size_of::<Signature>()].try_into().unwrap(),
        )
    }
}

impl ArchiveTableEntry {
    pub fn new(
        signature: Signature,
        name_offset: u64,
        name_len: u64,
        extra_data_offset: u64,
        extra_data_len: u64,
        data_offset: u64,
        data_len: u64,
    ) -> Self {
        Self {
            signature,
            name_offset,
            name_len,
            extra_data_offset,
            extra_data_len,
            data_offset,
            data_len,
        }
    }
}

pub struct Archive<'a> {
    header: &'a ArchiveHeader,
    entry_table: &'a [ArchiveTableEntry],
    names: &'a [u8],
    extra_data: &'a [u8],
    // TODO archive 'blocks' for easier streaming decompression
    data: &'a [u8],
}

impl core::ops::Deref for Archive<'_> {
    type Target = ArchiveHeader;

    fn deref(&self) -> &Self::Target {
        self.header
    }
}

pub struct ArchiveEntry<'a> {
    name: &'a str,
    extra_data: &'a [u8],
    data: &'a [u8],
}

impl<'a> ArchiveEntry<'a> {
    fn from_table_entry(
        table_entry: &'a ArchiveTableEntry,
        names: &'a [u8],
        extra_data: &'a [u8],
        data: &'a [u8],
    ) -> Self {
        let name_offset = usize::try_from(table_entry.name_offset).unwrap();
        let name_len = usize::try_from(table_entry.name_len).unwrap();
        let extra_data_offset = usize::try_from(table_entry.extra_data_offset).unwrap();
        let extra_data_len = usize::try_from(table_entry.extra_data_len).unwrap();
        let data_offset = usize::try_from(table_entry.data_offset).unwrap();
        let data_len = usize::try_from(table_entry.data_len).unwrap();

        let name_range = name_offset..(name_offset + name_len);
        let extra_data_range = extra_data_offset..(extra_data_offset + extra_data_len);
        let data_range = data_offset..(data_offset + data_len);

        let name_bytes = &names[name_range];
        let extra_data_bytes = &extra_data[extra_data_range];
        let data_bytes = &data[data_range];

        ArchiveEntry {
            name: core::str::from_utf8(name_bytes).expect("table entry has invalid UTF-8 bytes"),
            extra_data: extra_data_bytes,
            data: data_bytes,
        }
    }

    #[inline]
    pub const fn name(&self) -> &str {
        self.name
    }

    #[inline]
    pub const fn extra_data(&self) -> &[u8] {
        self.extra_data
    }

    #[inline]
    pub const fn data(&self) -> &[u8] {
        self.data
    }
}

pub struct ArchiveIterator<'a> {
    entries: &'a [ArchiveTableEntry],
    names: &'a [u8],
    extra_data: &'a [u8],
    data: &'a [u8],
    index: usize,
}

impl<'a> Iterator for ArchiveIterator<'a> {
    type Item = ArchiveEntry<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let table_entry = self.entries.get(self.index)?;
        self.index += 1;

        Some(ArchiveEntry::from_table_entry(
            table_entry,
            self.names,
            self.extra_data,
            self.data,
        ))
    }
}

impl DoubleEndedIterator for ArchiveIterator<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        let table_entry = self.entries.get(self.index)?;
        self.index -= 1;

        Some(ArchiveEntry::from_table_entry(
            table_entry,
            self.names,
            self.extra_data,
            self.data,
        ))
    }
}

impl ExactSizeIterator for ArchiveIterator<'_> {
    fn len(&self) -> usize {
        self.entries.len() - self.index
    }
}
