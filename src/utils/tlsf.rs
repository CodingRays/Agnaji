use std::num::NonZeroUsize;
use std::ptr::{NonNull, null_mut};

pub struct Allocation<T> {
    header: NonNull<BlockHeader<T>>,
}

impl<T> Allocation<T> {
    pub unsafe fn get_offset(&self) -> usize {
        self.header.as_ref().base_offset
    }

    pub unsafe fn get_pool(&self) -> &T {
        self.header.as_ref().pool.as_ref().unwrap()
    }
}

pub struct TLSF<T> {
    free_first_level_mask: usize,
    segregated_lists: Box<[Box<SecondLevel<T>>]>,
    header_free_list: *mut BlockHeader<T>,
    header_pool: Vec<Box<[BlockHeader<T>]>>,
    page_pool: Vec<Box<T>>,
}

impl<T> TLSF<T> {
    /// The number of blocks that are missing because of the min block size.
    const MISSING_MIN_BLOCKS: u32 = 5;
    const MIN_BLOCK_MASK: usize = (1usize << Self::MISSING_MIN_BLOCKS) - 1;

    /// The minimum block size
    pub const MIN_BLOCK_SIZE: usize = 1 << Self::MISSING_MIN_BLOCKS;

    const SECOND_LEVEL_INDEX: u32 = 5;

    pub fn new_for_max_size(max_block_size: usize) -> Self {
        let first_level_index = usize::BITS - max_block_size.trailing_zeros();
        let segregated_lists: Box<_> = std::iter::repeat_with(|| Box::new(SecondLevel::new()))
            .take((first_level_index - Self::MISSING_MIN_BLOCKS) as usize)
            .collect();

        Self {
            free_first_level_mask: 0,
            segregated_lists,
            header_free_list: null_mut(),
            header_pool: Vec::with_capacity(4),
            page_pool: Vec::with_capacity(4),
        }
    }

    pub unsafe fn allocate(&mut self, size: NonZeroUsize) -> Option<Allocation<T>> {
        let (first_level, second_level) = self.find_free_block_index(size)?;

        let mut header = self.take_block(
            first_level as usize,
            second_level as usize
        ).unwrap();

        let header_ref = header.as_mut();
        header_ref.clear_free_block_flag();

        let rounded_size = (size.get() + Self::MIN_BLOCK_MASK) & !Self::MIN_BLOCK_MASK;
        let split_size = header_ref.get_size() - rounded_size;
        if split_size > 0 {
            let mut split_block = self.allocate_block_header();
            let split_block_ref = split_block.as_mut();
            split_block_ref.set_free_block_flag();

            header_ref.set_size(rounded_size);
            split_block_ref.set_size(split_size);
            split_block_ref.base_offset = header_ref.base_offset + rounded_size;
            split_block_ref.pool = header_ref.pool;

            // This also modifies header!!!
            split_block_ref.insert_to_physical_list_after(header);
            self.return_block_no_merge(split_block);
        }

        Some(Allocation {
            header
        })
    }

    pub unsafe fn free(&mut self, allocation: Allocation<T>) {
        let mut header = allocation.header;

        let header_ref = header.as_ref();
        let mut size = header_ref.get_size();
        let mut base_offset = header_ref.base_offset;

        if let Some(prev) = header_ref.prev_physical.as_mut() {
            if prev.is_free_block() {
                prev.remove_from_free_list();
                prev.remove_from_physical_list();

                size += prev.get_size();
                base_offset = prev.base_offset;

                self.free_block_header(NonNull::from(prev));
            }
        }

        // Need to reborrow because potential write
        if let Some(next) = header.as_ref().next_physical.as_mut() {
            if next.is_free_block() {
                next.remove_from_free_list();
                next.remove_from_physical_list();

                size += next.get_size();

                self.free_block_header(NonNull::from(next));
            }
        }

        // Need to reborrow because potential write
        let header_ref = header.as_mut();
        header_ref.set_size(size);
        header_ref.base_offset = base_offset;

        self.return_block_no_merge(header)
    }

    pub unsafe fn new_page(&mut self, page: Box<T>, size: usize) {
        // TODO validate size range

        let ptr = page.as_ref() as *const T;

        self.page_pool.push(page);
        let mut header = self.allocate_block_header();

        let header_ref = header.as_mut();
        header_ref.make_new_physical_list();
        header_ref.set_free_block_flag();

        header_ref.set_size(size);
        header_ref.base_offset = 0;
        header_ref.pool = ptr;

        self.return_block_no_merge(header);
    }

    unsafe fn take_block(&mut self, first_level_index: usize, second_level_index: usize) -> Option<NonNull<BlockHeader<T>>> {
        let second_level = self.segregated_lists.get(first_level_index).unwrap();
        let block_header = second_level.list_headers.get(second_level_index).unwrap();

        if let Some(mut block_header) = NonNull::new(*block_header) {
            block_header.as_mut().remove_from_free_list();

            // We need to reborrow here because the second level would get modified by the remove so our old
            // reference would have been modified despite being borrowed
            let second_level = self.segregated_lists.get_mut(first_level_index).unwrap();
            if second_level.list_headers.get(second_level_index).unwrap().is_null() {
                second_level.free_mask &= !(1 << second_level_index);

                if second_level.free_mask == 0 {
                    self.free_first_level_mask &= !(1 << first_level_index);
                }
            }

            Some(block_header)
        } else {
            None
        }
    }

    unsafe fn return_block_no_merge(&mut self, mut block: NonNull<BlockHeader<T>>) {
        let size = block.as_ref().get_size();
        let (first_level, second_level) = Self::map_block_size(NonZeroUsize::new(size).unwrap());

        self.free_first_level_mask |= 1 << first_level;
        let second_level_info = self.segregated_lists.get_mut(first_level as usize).unwrap();
        second_level_info.free_mask |= 1 << second_level;
        let head = NonNull::from(second_level_info.list_headers.get(second_level as usize).unwrap());

        block.as_mut().insert_to_free_list_head(head);
    }

    unsafe fn allocate_block_header(&mut self) -> NonNull<BlockHeader<T>> {
        if let Some(header) = self.header_free_list.as_mut() {
            header.remove_from_free_list();
            NonNull::from(header)
        } else {
            let mut pool: Box<_> = std::iter::repeat_with(BlockHeader::new).take(64).collect();

            for header in &mut pool[1..] {
                header.insert_to_free_list_head(NonNull::from(&mut self.header_free_list));
            }
            let header = NonNull::from(&mut pool[0]);

            self.header_pool.push(pool);

            header
        }
    }

    unsafe fn free_block_header(&mut self, mut header: NonNull<BlockHeader<T>>) {
        header.as_mut().insert_to_free_list_head(NonNull::from(&mut self.header_free_list));
    }

    fn find_free_block_index(&self, size: NonZeroUsize) -> Option<(u32, u32)> {
        let (first_level, second_level) = Self::map_request_size(size);

        let mut selected_first_level = Self::first_one_after_at(
            self.free_first_level_mask,
            first_level
        )?;

        let selected_second_level;
        if first_level == selected_first_level {
            if let Some(free_second_level) = Self::first_one_after_at(
                self.segregated_lists.get(selected_first_level as usize).unwrap().free_mask as usize,
                second_level
            ) {
                selected_second_level = free_second_level;
            } else {
                if let Some(new_first_level) = Self::first_one_after_at(self.free_first_level_mask, first_level + 1) {
                    selected_first_level = new_first_level;

                    selected_second_level = Self::first_one_after_at(
                        self.segregated_lists.get(new_first_level as usize).unwrap().free_mask as usize,
                        0
                    ).unwrap(); // Must succeed because otherwise the first level bit would've been cleared
                } else {
                    return None;
                }
            }

        } else {
            selected_second_level = Self::first_one_after_at(
                self.segregated_lists.get(selected_first_level as usize).unwrap().free_mask as usize,
                0
            ).unwrap(); // Must succeed because otherwise the first level bit would've been cleared
        }

        Some((selected_first_level, selected_second_level))
    }

    fn map_request_size(size: NonZeroUsize) -> (u32, u32) {
        let last_bit = usize::BITS - size.trailing_zeros();
        let first_level = last_bit.saturating_sub(Self::MISSING_MIN_BLOCKS);

        let masked_size = size.get() & !(1 << last_bit);
        let second_level = (masked_size >> last_bit.saturating_sub(Self::SECOND_LEVEL_INDEX)) as u32;

        (first_level, second_level)
    }

    fn map_block_size(size: NonZeroUsize) -> (u32, u32) {
        let (first_level, second_level) = Self::map_request_size(size);

        let end_range = (1usize << (first_level + Self::SECOND_LEVEL_INDEX)) + ((1usize << first_level) * (second_level as usize));
        if size.get() == end_range {
            (first_level, second_level)
        } else {
            if second_level + 1 >= (1 << Self::SECOND_LEVEL_INDEX) {
                (first_level + 1, 0)
            } else {
                (first_level, second_level + 1)
            }
        }
    }

    #[inline(always)]
    fn first_one_after_at(mask: usize, after_at: u32) -> Option<u32> {
        let leading_zeros = (mask & ((1 << after_at) - 1)).leading_zeros();
        if leading_zeros < 32 {
            Some(leading_zeros)
        } else {
            None
        }
    }
}

struct SecondLevel<T> {
    free_mask: u32,
    list_headers: [*mut BlockHeader<T>; 32],
}

impl<T> SecondLevel<T> {
    fn new() -> Self {
        Self {
            free_mask: 0,
            list_headers: [null_mut(); 32],
        }
    }
}

/// The header for a block of memory.
///
/// Terminology note: When the term header is used it references an instance of this struct either
/// as part of the TLSF data structure or outside of it (for example as part of the free header
/// list).
/// When the term block is used it references the memory block this struct represents as part of the
/// TLSF data structure.
#[repr(C)]
struct BlockHeader<T> {
    /// The next free header in the free list. This pointer is used for both the free block list as
    /// well as the free header list.
    ///
    /// If this is the last header in the list this pointer is null.
    /// If this header is part of a free list and this pointer is not null then the pointer must
    /// point to a valid header and the `prev_free` field of that header must point to this header.
    ///
    /// If this header is not in a free list the value of this pointer is undefined.
    ///
    /// # Important
    /// If this is the first header in the free list the `prev_free` pointer points to the list head
    /// pointer. So to remove or insert a element into the free list we just reinterpret the
    /// `prev_free` pointer as `*mut *mut BlockHeader`. In order to enable this the `next_free`
    /// pointer **must** be the first field in this struct.
    next_free: *mut BlockHeader<T>,

    /// The previous free header in the free list or the list head pointer if this is the first
    /// header. This pointer is used for both the free block list as well as the free header list.
    ///
    /// If this header is part of a free list this pointer must be non null and if this header is
    /// not the first in the list (i.e. the first free block flag is cleared) it must be valid to
    /// reinterpret this pointer as `*mut BlockHeader`. In either case the referenced pointer must
    /// point to this header.
    ///
    /// If this header is not in a free list the value of this pointer is undefined.
    prev_free: *mut *mut BlockHeader<T>,

    next_physical: *mut BlockHeader<T>,
    prev_physical: *mut BlockHeader<T>,

    /// Pointer to the pool header that this block is a part of. Must be not null while this header
    /// is part of the TLSF list.
    ///
    /// If this header is part of the header free list the value of this pointer is undefined.
    pool: *const T,

    /// The size of this block. Also contains the free block flag and the first free block flag in
    /// its 2 least significant bits. As a result the size must always be a multiple of 4.
    ///
    /// The free block flag is only defined while the header is part of the TLSF list (i.e. not in
    /// the free header list). In that case it is set if the block is part of the free block list.
    ///
    /// The first free block flag is only defined while the header is part of a free list. Either
    /// the free block list or the free header list. In that case it is set if the `prev_free`
    /// points to the free list header pointer and not another header.
    ///
    /// **Note:** the flags may have **any** value outside of their defined scope.
    size_and_flags: usize,

    /// The offset into the pool memory where this block starts. This is relative to the pool
    /// memory. For example 0 means the start of the pool.
    base_offset: usize,
}

impl<T> BlockHeader<T> {
    const FREE_BLOCK_FLAG: usize = 0b01;
    const FIRST_FREE_BLOCK_FLAG: usize = 0b10;
    const BLOCK_SIZE_MASK: usize = !(Self::FREE_BLOCK_FLAG | Self::FIRST_FREE_BLOCK_FLAG);

    fn new() -> Self {
        Self {
            next_free: null_mut(),
            prev_free: null_mut(),
            next_physical: null_mut(),
            prev_physical: null_mut(),
            pool: null_mut(),
            size_and_flags: 0,
            base_offset: 0,
        }
    }

    /// Removes this header from a free list and updates **only** other structs in the same free
    /// list accordingly. If the free list was in a valid state before calling this function it will
    /// be in a valid state afterwards too.
    ///
    /// All fields of this header will remain untouched. In particular the free block flag must be
    /// manually set or cleared depending on how the header will be used. Since the free list
    /// pointers and first free block flags are undefined outside of a free list they are **not**
    /// cleared.
    ///
    /// # Safety
    /// This header must be part of a free list, either the header free list or the block free list.
    unsafe fn remove_from_free_list(&mut self) {
        // There must always be a prev_free as the first block has a pointer to the free list
        // head pointer.
        *self.prev_free = self.next_free;
        if let Some(next) = self.next_free.as_mut() {
            next.prev_free = self.prev_free;

            if self.is_first_free_block() {
                next.set_first_free_block_flag();
            }
        }

        // Note the first free block flag is undefined outside of a free list which is why we dont update it
    }

    /// Inserts this header into the head of a free list. The `head` parameter must be the head
    /// pointer of the free list. If the free list was in a valid state before calling this function
    /// it will be in a valid state afterwards too.
    ///
    /// Only modifies the free list pointers and first free block flag of this struct. All other
    /// fields will remain untouched. In particular the free block flag must be manually set or
    /// cleared depending on how the header was used.
    ///
    /// # Safety
    /// This header must not be currently part of a free list.
    unsafe fn insert_to_free_list_head(&mut self, mut head: NonNull<*mut BlockHeader<T>>) {
        self.set_first_free_block_flag();

        if let Some(next) = head.as_ref().as_mut() {
            next.prev_free = self as *mut BlockHeader<T> as *mut *mut BlockHeader<T>;
            next.clear_first_free_block_flag();
        }

        self.next_free = *head.as_ref();
        self.prev_free = head.as_ptr();

        *head.as_mut() = self;
    }

    /// Removes this header from the physical list and updates **only** other structs in the same
    /// physical list accordingly. If the physical list was in a valid state before calling this
    /// function it will be in a valid state afterwards too.
    ///
    /// All fields of this header will remain untouched. Since the physical list pointers are
    /// undefined outside of a physical list they are **not** cleared.
    ///
    /// # Safety
    /// This header must be part of a physical list.
    unsafe fn remove_from_physical_list(&mut self) {
        if let Some(prev) = self.prev_physical.as_mut() {
            prev.next_physical = self.next_physical;
        }
        if let Some(next) = self.next_physical.as_mut() {
            next.prev_physical = self.prev_physical;
        }
    }

    /// Inserts this header into a physical list after the specified header. Only updates the
    /// physical list pointers. All other fields of this header will remain untouched. If the
    /// physical list was in a valid state before calling this function it will be in a valid state
    /// afterwards too.
    ///
    /// # Safety
    /// This header must not be part of a physical list.
    unsafe fn insert_to_physical_list_after(&mut self, mut prev: NonNull<BlockHeader<T>>) {
        if let Some(next) = prev.as_ref().next_physical.as_mut() {
            next.prev_physical = self;
        }

        self.next_physical = prev.as_ref().next_physical;
        self.prev_physical = prev.as_ptr();

        prev.as_mut().next_physical = self;
    }

    /// Makes a new physical list containing only this header. This only updates the physical list
    /// pointers. All other fields of this header will remain untouched.
    ///
    /// # Safety
    /// This header must not be part of a physical list.
    unsafe fn make_new_physical_list(&mut self) {
        self.next_physical = null_mut();
        self.prev_physical = null_mut();
    }

    /// Returns the size field of this header.
    #[inline(always)]
    fn get_size(&self) -> usize {
        self.size_and_flags & Self::BLOCK_SIZE_MASK
    }

    /// Sets the size of this header. The size must be a multiple of 4.
    ///
    /// # Safety
    /// The size must not have any of the bits set in [`Self::BLOCK_SIZE_MASK`].
    #[inline(always)]
    unsafe fn set_size(&mut self, size: usize) {
        self.size_and_flags = size | (self.size_and_flags & !Self::BLOCK_SIZE_MASK);
    }

    #[inline(always)]
    fn is_free_block(&self) -> bool {
        self.size_and_flags & Self::FREE_BLOCK_FLAG != 0
    }

    #[inline(always)]
    fn set_free_block_flag(&mut self) {
        self.size_and_flags |= Self::FREE_BLOCK_FLAG;
    }

    #[inline(always)]
    fn clear_free_block_flag(&mut self) {
        self.size_and_flags &= !Self::FREE_BLOCK_FLAG;
    }

    #[inline(always)]
    fn is_first_free_block(&self) -> bool {
        self.size_and_flags & Self::FIRST_FREE_BLOCK_FLAG != 0
    }

    #[inline(always)]
    fn set_first_free_block_flag(&mut self) {
        self.size_and_flags |= Self::FIRST_FREE_BLOCK_FLAG;
    }

    #[inline(always)]
    fn clear_first_free_block_flag(&mut self) {
        self.size_and_flags &= !Self::FIRST_FREE_BLOCK_FLAG;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_header_free_insert_remove_1() {
        let mut list_header: *mut BlockHeader<()> = null_mut();
        let mut header = BlockHeader::new();

        for _ in 0..8 {
            unsafe {
                header.insert_to_free_list_head(NonNull::from(&mut list_header));
            }
            assert_eq!(list_header, &mut header as *mut BlockHeader<()>);
            assert_eq!(header.prev_free, &mut list_header as *mut *mut BlockHeader<()>);
            assert_eq!(header.next_free, null_mut());
            assert!(header.is_first_free_block());

            unsafe {
                header.remove_from_free_list();
            }
            assert_eq!(list_header, null_mut());
        }
    }

    #[test]
    fn block_header_free_insert_remove_2() {
        let mut list_header: *mut BlockHeader<()> = null_mut();
        let mut header1 = BlockHeader::new();
        let mut header2 = BlockHeader::new();

        for _ in 0..8 {
            unsafe {
                header1.insert_to_free_list_head(NonNull::from(&mut list_header));
                header2.insert_to_free_list_head(NonNull::from(&mut list_header));
            }
            assert_eq!(list_header, &mut header2 as *mut BlockHeader<()>);

            assert_eq!(header2.prev_free, &mut list_header as *mut *mut BlockHeader<()>);
            assert_eq!(header2.next_free, &mut header1 as *mut BlockHeader<()>);
            assert!(header2.is_first_free_block());

            assert_eq!(header1.prev_free, &mut header2 as *mut BlockHeader<()> as *mut *mut BlockHeader<()>);
            assert_eq!(header1.next_free, null_mut());
            assert!(!header1.is_first_free_block());

            unsafe {
                header1.remove_from_free_list();
            }
            assert_eq!(list_header, &mut header2 as *mut BlockHeader<()>);

            assert_eq!(header2.prev_free, &mut list_header as *mut *mut BlockHeader<()>);
            assert_eq!(header2.next_free, null_mut());
            assert!(header2.is_first_free_block());

            unsafe {
                header2.remove_from_free_list();
            }
            assert_eq!(list_header, null_mut());
        }
    }

    #[test]
    fn block_header_free_insert_remove_3() {
        let mut list_header: *mut BlockHeader<()> = null_mut();
        let mut header1 = BlockHeader::new();
        let mut header2 = BlockHeader::new();

        for _ in 0..8 {
            unsafe {
                header1.insert_to_free_list_head(NonNull::from(&mut list_header));
                header2.insert_to_free_list_head(NonNull::from(&mut list_header));
            }
            assert_eq!(list_header, &mut header2 as *mut BlockHeader<()>);

            assert_eq!(header2.prev_free, &mut list_header as *mut *mut BlockHeader<()>);
            assert_eq!(header2.next_free, &mut header1 as *mut BlockHeader<()>);
            assert!(header2.is_first_free_block());

            assert_eq!(header1.prev_free, &mut header2 as *mut BlockHeader<()> as *mut *mut BlockHeader<()>);
            assert_eq!(header1.next_free, null_mut());
            assert!(!header1.is_first_free_block());

            unsafe {
                header2.remove_from_free_list();
            }
            assert_eq!(list_header, &mut header1 as *mut BlockHeader<()>);

            assert_eq!(header1.prev_free, &mut list_header as *mut *mut BlockHeader<()>);
            assert_eq!(header1.next_free, null_mut());
            assert!(header1.is_first_free_block());

            unsafe {
                header1.remove_from_free_list();
            }
            assert_eq!(list_header, null_mut());
        }
    }
}