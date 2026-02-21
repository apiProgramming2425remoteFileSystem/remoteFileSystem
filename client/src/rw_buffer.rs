use std::fmt::{Debug, Formatter, Result as FmtResult};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use tracing::{Level, instrument};

pub struct ReadBuffer {
    path: PathBuf,
    offset: usize,
    valid_up_to: usize,
    filled_at: Instant,
    buffer: Vec<u8>,
    capacity: usize,
    ttl: Duration,
}

impl ReadBuffer {
    #[instrument(ret(level = Level::DEBUG))]
    pub fn new(capacity: usize) -> Self {
        ReadBuffer {
            path: PathBuf::new(),
            offset: 0,
            buffer: vec![0; capacity],
            filled_at: Instant::now(),
            ttl: Duration::from_millis(100),
            valid_up_to: 0,
            capacity,
        }
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    #[instrument(skip(self, data))]
    pub fn fill<P: AsRef<Path> + Debug>(&mut self, path: P, offset: usize, data: &[u8]) {
        self.path = path.as_ref().to_path_buf();
        self.offset = offset;
        self.filled_at = Instant::now();
        let to_copy = data.len().min(self.capacity);
        self.buffer[..to_copy].copy_from_slice(&data[..to_copy]);
        self.valid_up_to = to_copy;
    }

    #[instrument(skip(self))]
    pub fn read<P: AsRef<Path> + Debug>(&self, path: P, offset: usize, len: usize) -> Vec<u8> {
        if path.as_ref() != self.path
            || offset < self.offset
            || offset >= self.offset + self.valid_up_to
            || self.filled_at + self.ttl < Instant::now()
        {
            Vec::new()
        } else {
            let real_offset = offset - self.offset;
            let real_end = (real_offset + len).min(self.valid_up_to);
            self.buffer[real_offset..real_end].to_vec()
        }
    }
}

impl Debug for ReadBuffer {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_struct("ReadBuffer")
            .field("path", &self.path)
            .field("offset", &self.offset)
            .field("valid_up_to", &self.valid_up_to)
            .field("buffer", &"&[u8; ..]")
            .field("capacity", &self.capacity)
            .finish()
    }
}

pub struct WriteBuffer {
    path: PathBuf,
    offset: usize,
    valid_up_to: usize,
    buffer: Vec<u8>,
    capacity: usize,
}

impl WriteBuffer {
    #[instrument(ret(level = Level::DEBUG))]
    pub fn new(capacity: usize) -> Self {
        WriteBuffer {
            path: PathBuf::new(),
            offset: 0,
            buffer: vec![0; capacity],
            valid_up_to: 0,
            capacity,
        }
    }

    #[instrument(skip(self), ret(level = Level::DEBUG))]
    pub fn is_appending<P: AsRef<Path> + Debug>(&self, path: P, offset: usize) -> bool {
        self.path == path.as_ref() && self.offset + self.valid_up_to == offset
    }

    #[instrument(skip(self, data), ret(level = Level::DEBUG))]
    pub fn write<P: AsRef<Path> + Debug>(&mut self, path: P, offset: usize, data: &[u8]) -> usize {
        let path = path.as_ref();

        if self.is_appending(path, offset) {
            // append
            let available = self.capacity - self.valid_up_to;
            let to_copy = data.len().min(available);
            self.buffer[self.valid_up_to..self.valid_up_to + to_copy]
                .copy_from_slice(&data[..to_copy]);
            self.valid_up_to += to_copy;
            to_copy
        } else {
            self.path = path.to_path_buf();
            self.offset = offset;
            let to_copy = data.len().min(self.capacity);
            self.buffer[..to_copy].copy_from_slice(&data[..to_copy]);
            self.valid_up_to = to_copy;
            to_copy
        }
    }

    #[instrument(skip(self), ret(level = Level::DEBUG))]
    pub fn is_full(&self) -> bool {
        self.valid_up_to >= self.capacity
    }

    #[instrument(skip(self))]
    pub fn clean(&mut self) {
        self.path = PathBuf::new();
        self.offset = 0;
        self.valid_up_to = 0
    }

    #[instrument(skip(self))]
    pub fn get_content(&self) -> (&Path, usize, &[u8]) {
        (&self.path, self.offset, &self.buffer[..self.valid_up_to])
    }
}

impl Debug for WriteBuffer {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_struct("WriteBuffer")
            .field("path", &self.path)
            .field("offset", &self.offset)
            .field("valid_up_to", &self.valid_up_to)
            .field("buffer", &"&[u8; ..]")
            .field("capacity", &self.capacity)
            .finish()
    }
}
