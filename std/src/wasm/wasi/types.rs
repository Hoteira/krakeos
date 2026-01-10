#![allow(non_camel_case_types)]

pub type __wasi_errno_t = u16;
pub type __wasi_fd_t = u32;
pub type __wasi_filesize_t = u64;
pub type __wasi_timestamp_t = u64;
pub type __wasi_filedelta_t = i64;

pub const WASI_ESUCCESS: __wasi_errno_t = 0;
pub const WASI_E2BIG: __wasi_errno_t = 1;
pub const WASI_EACCES: __wasi_errno_t = 2;
pub const WASI_EADDRINUSE: __wasi_errno_t = 3;
pub const WASI_EADDRNOTAVAIL: __wasi_errno_t = 4;
pub const WASI_EAFNOSUPPORT: __wasi_errno_t = 5;
pub const WASI_EAGAIN: __wasi_errno_t = 6;
pub const WASI_EALREADY: __wasi_errno_t = 7;
pub const WASI_EBADF: __wasi_errno_t = 8;
pub const WASI_EBADMSG: __wasi_errno_t = 9;
pub const WASI_EBUSY: __wasi_errno_t = 10;
pub const WASI_ECANCELED: __wasi_errno_t = 11;
pub const WASI_ECHILD: __wasi_errno_t = 12;
pub const WASI_ECONNABORTED: __wasi_errno_t = 13;
pub const WASI_ECONNREFUSED: __wasi_errno_t = 14;
pub const WASI_ECONNRESET: __wasi_errno_t = 15;
pub const WASI_EDEADLK: __wasi_errno_t = 16;
pub const WASI_EDESTADDRREQ: __wasi_errno_t = 17;
pub const WASI_EDOM: __wasi_errno_t = 18;
pub const WASI_EDQUOT: __wasi_errno_t = 19;
pub const WASI_EEXIST: __wasi_errno_t = 20;
pub const WASI_EFAULT: __wasi_errno_t = 21;
pub const WASI_EFBIG: __wasi_errno_t = 22;
pub const WASI_EHOSTUNREACH: __wasi_errno_t = 23;
pub const WASI_EIDRM: __wasi_errno_t = 24;
pub const WASI_EILSEQ: __wasi_errno_t = 25;
pub const WASI_EINPROGRESS: __wasi_errno_t = 26;
pub const WASI_EINTR: __wasi_errno_t = 27;
pub const WASI_EINVAL: __wasi_errno_t = 28;
pub const WASI_EIO: __wasi_errno_t = 29;
pub const WASI_EISCONN: __wasi_errno_t = 30;
pub const WASI_EISDIR: __wasi_errno_t = 31;
pub const WASI_ELOOP: __wasi_errno_t = 32;
pub const WASI_EMFILE: __wasi_errno_t = 33;
pub const WASI_EMLINK: __wasi_errno_t = 34;
pub const WASI_EMSGSIZE: __wasi_errno_t = 35;
pub const WASI_EMULTIHOP: __wasi_errno_t = 36;
pub const WASI_ENAMETOOLONG: __wasi_errno_t = 37;
pub const WASI_ENETDOWN: __wasi_errno_t = 38;
pub const WASI_ENETRESET: __wasi_errno_t = 39;
pub const WASI_ENETUNREACH: __wasi_errno_t = 40;
pub const WASI_ENFILE: __wasi_errno_t = 41;
pub const WASI_ENOBUFS: __wasi_errno_t = 42;
pub const WASI_ENODEV: __wasi_errno_t = 43;
pub const WASI_ENOENT: __wasi_errno_t = 44;
pub const WASI_ENOEXEC: __wasi_errno_t = 45;
pub const WASI_ENOLCK: __wasi_errno_t = 46;
pub const WASI_ENOLINK: __wasi_errno_t = 47;
pub const WASI_ENOMEM: __wasi_errno_t = 48;
pub const WASI_ENOMSG: __wasi_errno_t = 49;
pub const WASI_ENOPROTOOPT: __wasi_errno_t = 50;
pub const WASI_ENOSPC: __wasi_errno_t = 51;
pub const WASI_ENOSYS: __wasi_errno_t = 52;
pub const WASI_ENOTCONN: __wasi_errno_t = 53;
pub const WASI_ENOTDIR: __wasi_errno_t = 54;
pub const WASI_ENOTEMPTY: __wasi_errno_t = 55;
pub const WASI_ENOTRECOVERABLE: __wasi_errno_t = 56;
pub const WASI_ENOTSOCK: __wasi_errno_t = 57;
pub const WASI_ENOTSUP: __wasi_errno_t = 58;
pub const WASI_ENOTTY: __wasi_errno_t = 59;
pub const WASI_ENXIO: __wasi_errno_t = 60;
pub const WASI_EOVERFLOW: __wasi_errno_t = 61;
pub const WASI_EOWNERDEAD: __wasi_errno_t = 62;
pub const WASI_EPERM: __wasi_errno_t = 63;
pub const WASI_EPIPE: __wasi_errno_t = 64;
pub const WASI_EPROTO: __wasi_errno_t = 65;
pub const WASI_EPROTONOSUPPORT: __wasi_errno_t = 66;
pub const WASI_EPROTOTYPE: __wasi_errno_t = 67;
pub const WASI_ERANGE: __wasi_errno_t = 68;
pub const WASI_EROFS: __wasi_errno_t = 69;
pub const WASI_ESPIPE: __wasi_errno_t = 70;
pub const WASI_ESRCH: __wasi_errno_t = 71;
pub const WASI_ESTALE: __wasi_errno_t = 72;
pub const WASI_ETIMEDOUT: __wasi_errno_t = 73;
pub const WASI_ETXTBSY: __wasi_errno_t = 74;
pub const WASI_EXDEV: __wasi_errno_t = 75;
pub const WASI_ENOTCAPABLE: __wasi_errno_t = 76;

pub type __wasi_filetype_t = u8;
pub const WASI_FILETYPE_UNKNOWN: __wasi_filetype_t = 0;
pub const WASI_FILETYPE_BLOCK_DEVICE: __wasi_filetype_t = 1;
pub const WASI_FILETYPE_CHARACTER_DEVICE: __wasi_filetype_t = 2;
pub const WASI_FILETYPE_DIRECTORY: __wasi_filetype_t = 3;
pub const WASI_FILETYPE_REGULAR_FILE: __wasi_filetype_t = 4;
pub const WASI_FILETYPE_SOCKET_DGRAM: __wasi_filetype_t = 5;
pub const WASI_FILETYPE_SOCKET_STREAM: __wasi_filetype_t = 6;
pub const WASI_FILETYPE_SYMBOLIC_LINK: __wasi_filetype_t = 7;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct __wasi_iovec_t {
    pub buf: u32,
    pub buf_len: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct __wasi_ciovec_t {
    pub buf: u32,
    pub buf_len: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct __wasi_fdstat_t {
    pub fs_filetype: __wasi_filetype_t,
    pub fs_flags: u16,
    pub fs_rights_base: u64,
    pub fs_rights_inheriting: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct __wasi_filestat_t {
    pub dev: u64,
    pub ino: u64,
    pub filetype: __wasi_filetype_t,
    pub nlink: u32,
    pub size: __wasi_filesize_t,
    pub atim: __wasi_timestamp_t,
    pub mtim: __wasi_timestamp_t,
    pub ctim: __wasi_timestamp_t,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct __wasi_prestat_t {
    pub tag: u8,
    pub u: __wasi_prestat_u,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union __wasi_prestat_u {
    pub dir: __wasi_prestat_dir_t,
}

impl core::fmt::Debug for __wasi_prestat_u {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        unsafe {
            write!(f, "{:?}", self.dir)
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct __wasi_prestat_dir_t {
    pub pr_name_len: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct __wasi_dirent_t {
    pub d_next: u64,
    pub d_ino: u64,
    pub d_namlen: u32,
    pub d_type: __wasi_filetype_t,
}