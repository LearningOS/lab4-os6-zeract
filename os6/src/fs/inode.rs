use easy_fs::{
    EasyFileSystem,
    Inode,
    DiskInodeType,
    DirEntry,
    DiskInode,
    DIRENT_SZ,
    block_cache_sync_all,
};
use crate::drivers::BLOCK_DEVICE;
use crate::sync::UPSafeCell;
use alloc::sync::Arc;
use lazy_static::*;
use bitflags::*;
use alloc::vec::Vec;
use super::File;
use crate::mm::UserBuffer;
use crate::fs::{Stat, StatMode};
use easy_fs::layout::NAME_LENGTH_LIMIT;
/// A wrapper around a filesystem inode
/// to implement File trait atop
pub struct OSInode {
    readable: bool,
    writable: bool,
    inner: UPSafeCell<OSInodeInner>,
}

/// The OS inode inner in 'UPSafeCell'
pub struct OSInodeInner {
    offset: usize,
    inode: Arc<Inode>,
}

impl OSInode {
    /// Construct an OS inode from a inode
    pub fn new(
        readable: bool,
        writable: bool,
        inode: Arc<Inode>,
    ) -> Self {
        Self {
            readable,
            writable,
            inner: unsafe { UPSafeCell::new(OSInodeInner {
                offset: 0,
                inode,
            })},
        }
    }
    /// Read all data inside a inode into vector
    pub fn read_all(&self) -> Vec<u8> {
        let mut inner = self.inner.exclusive_access();
        let mut buffer = [0u8; 512];
        let mut v: Vec<u8> = Vec::new();
        loop {
            let len = inner.inode.read_at(inner.offset, &mut buffer);
            if len == 0 {
                break;
            }
            inner.offset += len;
            v.extend_from_slice(&buffer[..len]);
        }
        v
    }
}

lazy_static! {
    /// The root of all inodes, or '/' in short
    pub static ref ROOT_INODE: Arc<Inode> = {
        let efs = EasyFileSystem::open(BLOCK_DEVICE.clone());
        Arc::new(EasyFileSystem::root_inode(&efs))
    };
}

/// List all files in the filesystems
pub fn list_apps() {
    println!("/**** APPS ****");
    for app in ROOT_INODE.ls() {
        println!("{}", app);
    }
    println!("**************/");
}

bitflags! {
    /// Flags for opening files
    pub struct OpenFlags: u32 {
        const RDONLY = 0;
        const WRONLY = 1 << 0;
        const RDWR = 1 << 1;
        const CREATE = 1 << 9;
        const TRUNC = 1 << 10;
    }
}

impl OpenFlags {
    /// Get the current read write permission on an inode
    /// does not check validity for simplicity
    /// returns (readable, writable)
    pub fn read_write(&self) -> (bool, bool) {
        if self.is_empty() {
            (true, false)
        } else if self.contains(Self::WRONLY) {
            (false, true)
        } else {
            (true, true)
        }
    }
}

/// Open a file by path
pub fn open_file(name: &str, flags: OpenFlags) -> Option<Arc<OSInode>> {
    let (readable, writable) = flags.read_write();
    if flags.contains(OpenFlags::CREATE) {
        if let Some(inode) = ROOT_INODE.find(name) {
            // clear size
            inode.clear();
            Some(Arc::new(OSInode::new(
                readable,
                writable,
                inode,
            )))
        } else {
            // create file
            ROOT_INODE.create(name)
                .map(|inode| {
                    Arc::new(OSInode::new(
                        readable,
                        writable,
                        inode,
                    ))
                })
        }
    } else {
        ROOT_INODE.find(name)
            .map(|inode| {
                if flags.contains(OpenFlags::TRUNC) {
                    inode.clear();
                }
                Arc::new(OSInode::new(
                    readable,
                    writable,
                    inode
                ))
            })
    }
}
pub fn linkat(old_name: &str, new_name: &str)->isize{

    if new_name == old_name{
        return -1;
    }
    if let Some(inode) = ROOT_INODE.find(old_name) {
        inode.modify_disk_inode(|disk_inode|{
            disk_inode.nlink += 1;
        });
        ROOT_INODE.modify_disk_inode(|disk_inode|{
            let number = ROOT_INODE.find_inode_id(old_name, disk_inode);
            let mut fs = ROOT_INODE.fs.lock();
            let mut dirent = DirEntry::new(new_name,number.unwrap());
            let file_count = (disk_inode.size as usize) / DIRENT_SZ;
            let new_size = (file_count + 1) * DIRENT_SZ;
            ROOT_INODE.increase_size(new_size as u32, disk_inode, &mut fs);
            disk_inode.write_at(file_count*DIRENT_SZ, dirent.as_bytes_mut(), &ROOT_INODE.block_device);
        });
        block_cache_sync_all();
        0
    }else{
        -1
    }

}
pub fn unlinkat(name:&str)->isize{
    if let Some(inode) = ROOT_INODE.find(name){
        inode.modify_disk_inode(|disk_inode|{
            if disk_inode.nlink==1{
                disk_inode.clear_size(&inode.block_device);
                ROOT_INODE.modify_disk_inode(|disk|{
                    let file_count = (disk.size as usize)/DIRENT_SZ;
                    let new_size = (file_count-1)*DIRENT_SZ;
                    disk.size = new_size as u32;
                });
                0
            }else{
                disk_inode.nlink -= 1;
                ROOT_INODE.modify_disk_inode(|disk|{
                    let file_count = (disk.size as usize)/DIRENT_SZ;
                    let mut dirent = DirEntry::empty();
                    for i in 0..file_count {
                        assert_eq!(
                            disk.read_at(
                                DIRENT_SZ * i,
                                dirent.as_bytes_mut(),
                                &ROOT_INODE.block_device,
                            ),
                            DIRENT_SZ,
                        );
                        if dirent.name() == name {
                            let mut empty_dirent = DirEntry::empty();
                            disk.write_at(DIRENT_SZ * i, empty_dirent.as_bytes_mut(), &ROOT_INODE.block_device);
                            return 0;
                        }
                    }
                    //let new_size = (file_count-1)*DIRENT_SZ;
                    //disk.size = new_size as u32;
                    -1
                });
                0
            }
        })
    }else{
        -1
    }
}
impl File for OSInode {
    fn readable(&self) -> bool { self.readable }
    fn writable(&self) -> bool { self.writable }
    fn stat(&self,st:*mut Stat){
        let mut inner = self.inner.exclusive_access();
        let fs = inner.inode.fs.lock();
        inner.inode.read_disk_inode(|disk_inode|{
            unsafe{
                if disk_inode.is_dir(){
                    (*st).mode = StatMode::DIR;
                }else{
                    (*st).mode = StatMode::FILE;
                }
                (*st).nlink = disk_inode.nlink;
                (*st).dev = inner.inode.block_id as u64;
                (*st).ino = inner.inode.block_offset as u64;
                
            }
        })
    }
    fn read(&self, mut buf: UserBuffer) -> usize {
        let mut inner = self.inner.exclusive_access();
        let mut total_read_size = 0usize;
        for slice in buf.buffers.iter_mut() {
            let read_size = inner.inode.read_at(inner.offset, *slice);
            if read_size == 0 {
                break;
            }
            inner.offset += read_size;
            total_read_size += read_size;
        }
        total_read_size
    }
    fn write(&self, buf: UserBuffer) -> usize {
        let mut inner = self.inner.exclusive_access();
        let mut total_write_size = 0usize;
        for slice in buf.buffers.iter() {
            let write_size = inner.inode.write_at(inner.offset, *slice);
            assert_eq!(write_size, slice.len());
            inner.offset += write_size;
            total_write_size += write_size;
        }
        total_write_size
    }
}
