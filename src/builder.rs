use crate::{ArchiveHeader, ArchiveTableEntry, Signature};
use alloc::vec::Vec;

pub struct ArchiveBuilder {
    names: Vec<u8>,
    extra_data: Vec<u8>,
    data: Vec<u8>,
    table_entries: Vec<ArchiveTableEntry>,
}

impl ArchiveBuilder {
    pub const fn new() -> Self {
        ArchiveBuilder {
            names: Vec::new(),
            extra_data: Vec::new(),
            data: Vec::new(),
            table_entries: Vec::new(),
        }
    }

    pub fn push_entry(&mut self, signature: Signature, name: &str, extra_data: &[u8], data: &[u8]) {
        let name_offset = self.names.len().try_into().unwrap();
        let name_len = name.len().try_into().unwrap();
        self.names.extend_from_slice(name.as_bytes());

        let extra_data_offset = self.extra_data.len().try_into().unwrap();
        let extra_data_len = extra_data.len().try_into().unwrap();
        self.extra_data.extend_from_slice(extra_data);

        let data_offset = self.data.len().try_into().unwrap();
        let data_len = data.len().try_into().unwrap();
        self.data.extend_from_slice(data);

        self.table_entries.push(ArchiveTableEntry::new(
            signature,
            name_offset,
            name_len,
            extra_data_offset,
            extra_data_len,
            data_offset,
            data_len,
        ));
    }

    pub fn finish(self) -> Vec<u8> {
        let names_bytes = self.names.as_slice();
        let extra_data_bytes = self.extra_data.as_slice();
        let entries_bytes = bytemuck::cast_slice(self.table_entries.as_slice());
        let data_bytes = self.data.as_slice();

        let mut archive_bytes = Vec::new();

        let header_size = u64::try_from(core::mem::size_of::<ArchiveHeader>()).unwrap();
        let names_size = u64::try_from(self.names.len()).unwrap();
        let extra_data_size = u64::try_from(self.extra_data.len()).unwrap();
        let entries_size = u64::try_from(entries_bytes.len()).unwrap();
        let data_size = u64::try_from(data_bytes.len()).unwrap();
        let total_size = header_size + names_size + extra_data_size + entries_size + data_size;

        let header = ArchiveHeader::new(
            crate::VERSION_0_0_1_0,
            self.table_entries.len().try_into().unwrap(),
            self.names.len().try_into().unwrap(),
            self.extra_data.len().try_into().unwrap(),
            total_size,
        );

        archive_bytes.extend_from_slice(bytemuck::bytes_of(&header));
        archive_bytes.extend_from_slice(entries_bytes);
        archive_bytes.extend_from_slice(names_bytes);
        archive_bytes.extend_from_slice(extra_data_bytes);
        archive_bytes.extend_from_slice(data_bytes);

        assert_eq!(total_size, archive_bytes.len() as u64);

        archive_bytes
    }
}
