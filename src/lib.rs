#![cfg_attr(not(test), no_std)]
#![feature(
    error_in_core,
    array_chunks                    // #74985 <https://github.com/rust-lang/rust/issues/74985>
)]

use core::{mem::size_of, ptr::NonNull};

pub const MAGIC: &[u8; 8] = b"OARCHIVE";
pub const VERSIONS: [u32; 1] = [u32::from_le_bytes([0, 0, 1, 0])];

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

impl<'a> Archive<'a> {
    pub fn from_bytes(data: &'a [u8]) -> Result<Self> {
        // TODO pre-handle panic conditions for `split_at`

        let (header_bytes, data) = data.split_at(size_of::<ArchiveHeader>());
        let header = <&ArchiveHeader>::try_from(header_bytes)?;

        let entry_table_size =
            usize::try_from(header.entry_count).unwrap() * size_of::<ArchiveTableEntry>();
        let (entry_table_bytes, data) = data.split_at(entry_table_size);
        let entry_table: &[ArchiveTableEntry] =
            bytemuck::checked::try_cast_slice(entry_table_bytes)
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
