use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct ReadBuffer{
    path: PathBuf,
    offset: usize,
    buffer: Vec<u8>,
    capacity: usize,
}

impl ReadBuffer{
    pub fn new(capacity: usize) -> Self {
        ReadBuffer{ path: PathBuf::new(), offset: 0, buffer: Vec::with_capacity(capacity), capacity }
    }

    pub fn fill(&mut self, path: &Path, offset: usize, data: &[u8]){
        self.path = path.to_path_buf();
        self.offset = offset;
        self.buffer.clear();
        let to_copy = data.len().min(self.capacity);
        self.buffer.extend_from_slice(&data[..to_copy]);
    }

    pub fn read(&self, path: &Path, offset: usize, len: usize) -> Vec<u8> {
        if path != self.path{
            Vec::new()
        }
        else if offset < self.offset || offset >= self.offset + self.buffer.len(){
            Vec::new()
        }
        else {
            let real_offset = offset - self.offset;
            let real_end = (real_offset + len).min(self.buffer.len());
            self.buffer[real_offset..real_end].to_vec()
        }
    }
}

#[derive(Debug)]
pub struct WriteBuffer{
    path: PathBuf,
    offset: usize,
    buffer: Vec<u8>,
    capacity: usize,
}

impl WriteBuffer{
    pub fn new(capacity: usize) -> Self {
        WriteBuffer{ path: PathBuf::new(), offset: 0, buffer: Vec::with_capacity(capacity), capacity }
    }

    pub fn is_appending(&self, path: &Path, offset: usize) -> bool{
        self.path == path.to_path_buf() && self.offset + self.buffer.len() == offset
    }

    pub fn write(&mut self, path: &Path, offset: usize, data: &[u8]) -> usize {
        if self.is_appending(path, offset) {
            // append
            let available = self.capacity - self.buffer.len();
            let to_copy = data.len().min(available);
            self.buffer.extend_from_slice(&data[..to_copy]);
            to_copy
        }
        else {
            self.path = path.to_path_buf();
            self.offset = offset;
            self.buffer.clear();
            let to_copy = data.len().min(self.capacity);
            self.buffer.extend_from_slice(&data[..to_copy]);
            to_copy
        }
    }

    pub fn is_full(&self) -> bool{
        self.buffer.len() >= self.capacity
    }

    pub fn clean(&mut self) {
        self.path = PathBuf::new();
        self.offset = 0;
        self.buffer.clear();
    }

    pub fn get_content(&self) -> (&Path, usize, &[u8]) {
        (&self.path, self.offset, &self.buffer)
    }
}
