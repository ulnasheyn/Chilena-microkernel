//! Filesystem for Chilena
//!
//! Minimal implementation: simple in-memory filesystem.
//! Sufficient for boot scripts, shell, and basic userspace.
//!
//! A full disk-based filesystem can be developed later.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::RwLock;

// ---------------------------------------------------------------------------
// I/O Traits
// ---------------------------------------------------------------------------

/// Event type for poll syscall
#[derive(Clone, Copy, Debug)]
pub enum PollEvent {
    Read,
    Write,
}

/// All "files" or "devices" must implement this trait
pub trait FileIO: Send + Sync {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, ()>;
    fn write(&mut self, buf: &[u8])    -> Result<usize, ()>;
    fn close(&mut self);
    fn poll(&mut self, event: PollEvent) -> bool;
    fn kind(&self) -> u8 { 0 }
}

// ---------------------------------------------------------------------------
// Handle / Resource
// ---------------------------------------------------------------------------

use crate::sys::console::Console;

#[derive(Clone, Debug)]
pub enum Device {
    Console(Console),
    Null,
}

impl FileIO for Device {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, ()> {
        match self { Device::Console(c) => c.read(buf), Device::Null => Ok(0) }
    }
    fn write(&mut self, buf: &[u8]) -> Result<usize, ()> {
        match self { Device::Console(c) => c.write(buf), Device::Null => Ok(buf.len()) }
    }
    fn close(&mut self) {}
    fn poll(&mut self, e: PollEvent) -> bool {
        match self { Device::Console(c) => c.poll(e), Device::Null => false }
    }
    fn kind(&self) -> u8 { 1 }
}

#[derive(Clone, Debug)]
pub struct MemFile {
    data:   Vec<u8>,
    cursor: usize,
}

impl MemFile {
    fn new(data: Vec<u8>) -> Self { Self { data, cursor: 0 } }
    pub fn size(&self) -> usize   { self.data.len() }
}

impl FileIO for MemFile {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, ()> {
        let remaining = &self.data[self.cursor..];
        let n = remaining.len().min(buf.len());
        buf[..n].copy_from_slice(&remaining[..n]);
        self.cursor += n;
        Ok(n)
    }
    fn write(&mut self, buf: &[u8]) -> Result<usize, ()> {
        self.data.extend_from_slice(buf);
        Ok(buf.len())
    }
    fn close(&mut self) {}
    fn poll(&mut self, e: PollEvent) -> bool {
        match e {
            PollEvent::Read  => self.cursor < self.data.len(),
            PollEvent::Write => true,
        }
    }
    fn kind(&self) -> u8 { 0 }
}

#[derive(Clone, Debug)]
pub enum Resource {
    Device(Device),
    File(MemFile),
}

impl Resource {
    pub fn read(&mut self, buf: &mut [u8]) -> Result<usize, ()> {
        match self { Resource::Device(d) => d.read(buf), Resource::File(f) => f.read(buf) }
    }
    pub fn write(&mut self, buf: &[u8]) -> Result<usize, ()> {
        match self { Resource::Device(d) => d.write(buf), Resource::File(f) => f.write(buf) }
    }
    pub fn close(&mut self) {
        match self { Resource::Device(d) => d.close(), Resource::File(f) => f.close() }
    }
    pub fn poll(&mut self, e: PollEvent) -> bool {
        match self { Resource::Device(d) => d.poll(e), Resource::File(f) => f.poll(e) }
    }
    pub fn kind(&self) -> u8 {
        match self { Resource::Device(d) => d.kind(), Resource::File(f) => f.kind() }
    }
    pub fn size(&self) -> usize {
        match self { Resource::File(f) => f.size(), _ => 0 }
    }
}

// File handle type alias
pub type FileHandle = Resource;

// ---------------------------------------------------------------------------
// In-memory VFS (Virtual File System)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct FileInfo {
    pub size:    usize,
    pub is_dir:  bool,
    pub name:    String,
}

type Vfs = BTreeMap<String, Vec<u8>>;

lazy_static::lazy_static! {
    static ref VFS: RwLock<Vfs> = RwLock::new(BTreeMap::new());
    static ref MOUNTED: spin::Once<()> = spin::Once::new();
}

// ---------------------------------------------------------------------------
// Public filesystem API
// ---------------------------------------------------------------------------

pub fn is_mounted() -> bool {
    MOUNTED.get().is_some()
}

pub fn mount_memfs() {
    MOUNTED.call_once(|| ());
    klog!("FS: MemFS mounted");
}

pub fn exists(path: &str) -> bool {
    VFS.read().contains_key(path)
}

pub fn canonicalize(path: &str) -> Result<String, ()> {
    // Simple implementation: normalize slashes
    let canonical = if path.starts_with('/') {
        path.to_string()
    } else {
        let cwd = crate::sys::process::cwd();
        if cwd.ends_with('/') {
            alloc::format!("{}{}", cwd, path)
        } else {
            alloc::format!("{}/{}", cwd, path)
        }
    };
    Ok(canonical)
}

pub fn open_file(path: &str) -> Option<MemFile> {
    VFS.read().get(path).map(|data| MemFile::new(data.clone()))
}

pub fn open_resource(path: &str, _flags: u8) -> Option<Resource> {
    VFS.read().get(path).map(|data| Resource::File(MemFile::new(data.clone())))
}

pub fn stat(path: &str) -> Option<FileInfo> {
    VFS.read().get(path).map(|data| FileInfo {
        size:   data.len(),
        is_dir: false,
        name:   path.rsplit('/').next().unwrap_or(path).to_string(),
    })
}

pub fn write_file(path: &str, data: &[u8]) -> Result<(), ()> {
    VFS.write().insert(path.to_string(), data.to_vec());
    Ok(())
}

pub fn remove(path: &str) -> Result<(), ()> {
    VFS.write().remove(path).map(|_| ()).ok_or(())
}

/// Called during sys::mem::init
pub fn init() {
    mount_memfs();

    // Write default boot script if it doesn't exist
    if !exists("/ini/boot.sh") {
        write_file("/ini/boot.sh", b"shell\n").ok();
    }
}
