use crate::alloc::vec::Vec;

pub type FuncAddr = usize;
pub type TableAddr = usize;
pub type MemAddr = usize;
pub type GlobalAddr = usize;
pub type ElemAddr = usize;
pub type DataAddr = usize;
pub type ModuleAddr = usize;
pub type ComponentInstAddr = usize;
pub type ComponentFuncAddr = usize;

#[derive(Debug)]
pub struct AddrVec<A, T> {
    pub(crate) data: Vec<T>,
    _phantom: core::marker::PhantomData<A>,
}

impl<A, T> AddrVec<A, T> {
    pub fn new() -> Self {
        Self { data: Vec::new(), _phantom: core::marker::PhantomData }
    }
    pub fn push(&mut self, val: T) -> usize {
        let addr = self.data.len();
        self.data.push(val);
        addr
    }
    pub fn insert(&mut self, val: T) -> usize {
        self.push(val)
    }
    pub fn get(&self, addr: usize) -> &T {
        &self.data[addr]
    }
    pub fn get_mut(&mut self, addr: usize) -> &mut T {
        &mut self.data[addr]
    }
    pub fn first(&self) -> Option<&T> {
        self.data.first()
    }
    pub fn get_two_mut(&mut self, addr1: usize, addr2: usize) -> Option<(&mut T, &mut T)> {
        if addr1 == addr2 { return None; }
        if addr1 < addr2 {
            let (left, right) = self.data.split_at_mut(addr2);
            if addr1 >= left.len() { return None; }
            Some((&mut left[addr1], &mut right[0]))
        } else {
            let (left, right) = self.data.split_at_mut(addr1);
            if addr2 >= left.len() { return None; }
            Some((&mut right[0], &mut left[addr2]))
        }
    }
    pub fn len(&self) -> usize { self.data.len() }
    pub fn iter(&self) -> core::slice::Iter<'_, T> { self.data.iter() }
    pub fn iter_enumerated(&self) -> core::iter::Enumerate<core::slice::Iter<'_, T>> {
        self.data.iter().enumerate()
    }
}

impl<A, T> Default for AddrVec<A, T> {
    fn default() -> Self { Self::new() }
}

pub trait IntoAddr {
    fn into_addr(self) -> usize;
}

impl IntoAddr for usize {
    fn into_addr(self) -> usize { self }
}
