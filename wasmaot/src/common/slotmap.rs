use alloc::vec::Vec;
use core::{marker::PhantomData, mem};

enum SlotContent<T> {
    Unoccupied { prev_unoccupied: Option<usize> },
    Occupied { item: T },
}

struct Slot<T> {
    generation: u64,
    content: SlotContent<T>,
}

pub struct SlotMap<T> {
    slots: Vec<Slot<T>>,
    last_unoccupied: Option<usize>,
}

impl<T> Default for SlotMap<T> {
    fn default() -> Self {
        Self {
            slots: Vec::new(),
            last_unoccupied: None,
        }
    }
}

#[derive(Debug)]
pub struct SlotMapKey<T> {
    pub(crate) index: usize,
    pub(crate) generation: u64,
    pub(crate) phantom: PhantomData<T>,
}

impl<T> SlotMap<T> {
    pub fn insert(&mut self, item: T) -> SlotMapKey<T> {
        match self.last_unoccupied {
            Some(last_unoccupied) => {
                let slot = &mut self.slots[last_unoccupied];
                let SlotContent::Unoccupied { prev_unoccupied } = slot.content else {
                    unreachable!("last unoccupied slot in slotmap must be unoccupied")
                };
                self.last_unoccupied = prev_unoccupied;
                slot.content = SlotContent::Occupied { item };
                SlotMapKey {
                    index: last_unoccupied,
                    generation: slot.generation,
                    phantom: PhantomData,
                }
            }
            None => {
                let index = self.slots.len();
                let generation = 0;
                self.slots.push(Slot {
                    generation,
                    content: SlotContent::Occupied { item },
                });
                SlotMapKey {
                    index,
                    generation,
                    phantom: PhantomData,
                }
            }
        }
    }
    #[allow(unused)]
    pub fn get(&self, key: &SlotMapKey<T>) -> Option<&T> {
        let slot = self.slots.get(key.index)?;
        if slot.generation != key.generation {
            return None;
        }
        match &slot.content {
            SlotContent::Occupied { item } => Some(item),
            SlotContent::Unoccupied { .. } => None,
        }
    }
    pub fn get_mut(&mut self, key: &SlotMapKey<T>) -> Option<&mut T> {
        let slot = self.slots.get_mut(key.index)?;
        if slot.generation != key.generation {
            return None;
        }
        match &mut slot.content {
            SlotContent::Occupied { item } => Some(item),
            SlotContent::Unoccupied { .. } => None,
        }
    }
    pub fn remove(&mut self, key: &SlotMapKey<T>) -> Option<T> {
        let slot = self.slots.get_mut(key.index)?;
        if slot.generation != key.generation
            || matches!(slot.content, SlotContent::Unoccupied { .. })
        {
            return None;
        }
        let new_slot = if let Some(generation) = slot.generation.checked_add(1) {
            let prev_unoccupied = self.last_unoccupied;
            self.last_unoccupied = Some(key.index);
            Slot {
                generation,
                content: SlotContent::Unoccupied { prev_unoccupied },
            }
        } else {
            Slot {
                generation: 0,
                content: SlotContent::Unoccupied {
                    prev_unoccupied: None,
                },
            }
        };
        let previous_slot = mem::replace(slot, new_slot);
        let SlotContent::Occupied { item } = previous_slot.content else {
            unreachable!("slot was full")
        };
        Some(item)
    }
}
