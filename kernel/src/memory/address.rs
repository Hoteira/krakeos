use core::fmt;
use core::ops::{Add, AddAssign, Sub, SubAssign};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct PhysAddr(u64);

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct VirtAddr(u64);

impl PhysAddr {
    pub const fn new(addr: u64) -> Self {
        PhysAddr(addr)
    }

    pub fn as_u64(self) -> u64 {
        self.0
    }

    pub fn is_aligned(self, align: u64) -> bool {
        self.0 % align == 0
    }

    pub fn align_up(self, align: u64) -> Self {
        PhysAddr((self.0 + align - 1) & !(align - 1))
    }

    pub fn align_down(self, align: u64) -> Self {
        PhysAddr(self.0 & !(align - 1))
    }
}

impl VirtAddr {
    pub const fn new(addr: u64) -> Self {
        VirtAddr(addr)
    }

    pub fn as_u64(self) -> u64 {
        self.0
    }

    pub fn as_ptr<T>(self) -> *const T {
        self.0 as *const T
    }

    pub fn as_mut_ptr<T>(self) -> *mut T {
        self.0 as *mut T
    }

    pub fn is_aligned(self, align: u64) -> bool {
        self.0 % align == 0
    }

    pub fn align_up(self, align: u64) -> Self {
        VirtAddr((self.0 + align - 1) & !(align - 1))
    }

    pub fn align_down(self, align: u64) -> Self {
        VirtAddr(self.0 & !(align - 1))
    }
}

impl fmt::Debug for PhysAddr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "PhysAddr({:#x})", self.0)
    }
}

impl fmt::Debug for VirtAddr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "VirtAddr({:#x})", self.0)
    }
}

impl Add<u64> for PhysAddr {
    type Output = Self;
    fn add(self, rhs: u64) -> Self::Output {
        PhysAddr(self.0 + rhs)
    }
}

impl Add<u64> for VirtAddr {
    type Output = Self;
    fn add(self, rhs: u64) -> Self::Output {
        VirtAddr(self.0 + rhs)
    }
}

impl Sub<u64> for PhysAddr {
    type Output = Self;
    fn sub(self, rhs: u64) -> Self::Output {
        PhysAddr(self.0 - rhs)
    }
}

impl Sub<PhysAddr> for PhysAddr {
    type Output = u64;
    fn sub(self, rhs: PhysAddr) -> Self::Output {
        self.0 - rhs.0
    }
}
