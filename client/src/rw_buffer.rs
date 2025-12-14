use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct ReadBuffer{
    path: PathBuf,
    offset: usize,
    valid_up_to : usize,
    buffer: Vec<u8>,
    capacity: usize,
}

impl ReadBuffer{
    pub fn new(capacity: usize) -> Self {
        ReadBuffer{ path: PathBuf::new(), offset: 0, buffer: vec![0; capacity], valid_up_to:0, capacity }
    }

    pub fn fill(&mut self, path: &Path, offset: usize, data: &[u8]){
        self.path = path.to_path_buf();
        self.offset = offset;
        let to_copy = data.len().min(self.capacity);
        self.buffer[..to_copy].copy_from_slice(&data[..to_copy]);
        self.valid_up_to = to_copy;
    }

    pub fn read(&self, path: &Path, offset: usize, len: usize) -> Vec<u8> {
        if path != self.path{
            Vec::new()
        }
        else if offset < self.offset || offset >= self.offset + self.valid_up_to {
            Vec::new()
        }
        else {
            let real_offset = offset - self.offset;
            let real_end = (real_offset + len).min(self.valid_up_to);
            self.buffer[real_offset..real_end].to_vec()
        }
    }
}

#[derive(Debug)]
pub struct WriteBuffer{
    path: PathBuf,
    offset: usize,
    valid_up_to: usize,
    buffer: Vec<u8>,
    capacity: usize,
}

impl WriteBuffer{
    pub fn new(capacity: usize) -> Self {
        WriteBuffer{ path: PathBuf::new(), offset: 0, buffer: vec![0; capacity], valid_up_to:0, capacity }
    }

    pub fn is_appending(&self, path: &Path, offset: usize) -> bool{
        self.path == path && self.offset + self.valid_up_to == offset
    }

    pub fn write(&mut self, path: &Path, offset: usize, data: &[u8]) -> usize {
        if self.is_appending(path, offset) {
            // append
            let available = self.capacity - self.valid_up_to;
            let to_copy = data.len().min(available);
            self.buffer[self.valid_up_to..self.valid_up_to+to_copy].copy_from_slice(&data[..to_copy]);
            self.valid_up_to += to_copy;
            to_copy
        }
        else {
            self.path = path.to_path_buf();
            self.offset = offset;
            let to_copy = data.len().min(self.capacity);
            self.buffer[..to_copy].copy_from_slice(&data[..to_copy]);
            self.valid_up_to = to_copy;
            to_copy
        }
    }

    pub fn is_full(&self) -> bool{
        self.valid_up_to >= self.capacity
    }

    pub fn clean(&mut self) {
        self.path = PathBuf::new();
        self.offset = 0;
        self.valid_up_to = 0
    }

    pub fn get_content(&self) -> (&Path, usize, &[u8]) {
        (&self.path, self.offset, &self.buffer[..self.valid_up_to])
    }
}
