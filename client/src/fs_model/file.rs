use std::collections::HashMap;
use std::ffi::OsString;
use std::fmt;
use std::fmt::Debug;
use crate::fs_model::attributes::FileAttr;
use std::vec::Vec;
use crate::cache::Cache;

const PAGE_SIZE: usize = 4096;

#[derive(Clone, Debug)]
pub struct FilePage {
    pub content: Vec<u8>,     // PAGE_SIZE
    pub valid_up_to: usize,   // 0..=PAGE_SIZE
    pub valid_from: usize,
}

impl FilePage {
    pub fn new() -> Self {
        FilePage {
            content: vec![0u8; PAGE_SIZE],
            valid_up_to: 0,
            valid_from: PAGE_SIZE,
        }
    }

    pub fn write(&mut self, data: &[u8], offset: usize) {


        let end = offset + data.len();
        let real_end = end.min(PAGE_SIZE);
        if offset > real_end {
            return;
        }

        self.content[offset..real_end].copy_from_slice(data);
        self.valid_up_to = self.valid_up_to.max(real_end);
        self.valid_from = self.valid_from.min(offset);
    }

    pub fn read(&self, offset: usize, size: usize) -> Option<&[u8]> {
        let end = (offset + size).min(PAGE_SIZE);
        let real_end = end.min(self.valid_up_to);
        if offset < self.valid_from {
            return None;
        }
        if offset > real_end {
            return None
        }
        Some(&self.content[offset..real_end])
    }

}

#[derive(Clone)]
pub struct File {
    pub name: OsString,
    pub attributes: Option<FileAttr>,
    pub content: HashMap<u64, FilePage>,
}


impl File {
    pub fn new(name: OsString, attributes: Option<FileAttr>) -> Self {
        File{ name, attributes, content: HashMap::new() }
    }

    pub fn write_content(&mut self, offset: usize, data: &[u8]) {
        let mut remaining = data;
        let mut curr_offset = offset;

        while !remaining.is_empty() {
            let page_index = (curr_offset / PAGE_SIZE) as u64;
            let page_offset = curr_offset % PAGE_SIZE;

            let page = self.content
                .entry(page_index)
                .or_insert_with(FilePage::new);

            let writable = PAGE_SIZE - page_offset;
            let to_write = remaining.len().min(writable);

            page.write(&remaining[..to_write], page_offset);

            remaining = &remaining[to_write..];
            curr_offset += to_write;
        }
    }

    pub fn read(&self, offset: usize, size: usize) -> Vec<u8> {
        let mut buffer = Vec::with_capacity(size);
        let mut remaining = size;
        let mut curr_offset = offset;

        while remaining > 0 {
            let page_index = (curr_offset / PAGE_SIZE) as u64;
            let page_offset = curr_offset % PAGE_SIZE;
            let page = match self.content.get(&page_index) {
                Some(p) => p,
                None => break,
            };

            let max_read = remaining.min(PAGE_SIZE - page_offset);
            match page.read(page_offset, max_read) {
                Some(slice) => {
                    buffer.extend_from_slice(slice);
                    let read_now = slice.len();
                    remaining -= read_now;

                    if read_now < max_read {
                        break;
                    }
                }
                None => break,
            }

            curr_offset += max_read;
        }

        let read_bytes = buffer.len();
        buffer.resize(read_bytes, 0);
        buffer
    }

    pub fn merge(&mut self, other: File) {
        if other.attributes.is_some() {
            self.attributes = other.attributes;
        }
        for key in other.content.keys(){
            if let Some(page) = other.content.get(key) {
                self.write_content((*key as usize) * PAGE_SIZE + page.valid_from,
                                   &page.content[page.valid_from..page.valid_up_to]);
            }
        }
    }
}


impl Debug for File{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut result = String::from("attributes: ");
        if self.attributes.is_some(){
            result += "present  ";
        }
        else {
            result += "missing  ";
        }
        result += "pages: ";
        for key in self.content.keys(){
            if let Some(page) = self.content.get(key) {
                result += &format!("{}:[{}-{}]<{:?}> ", key, page.valid_from, page.valid_up_to, &page.content[page.valid_from..page.valid_up_to]);
            }
        }
        write!(f, "{}", result)
    }
}
