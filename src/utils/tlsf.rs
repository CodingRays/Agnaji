use std::alloc::Layout;
use std::mem::MaybeUninit;
use std::ptr::{NonNull, null_mut};
use static_assertions::assert_eq_size;

pub struct MultiPoolTLSF {
    header_free_list_head: *mut BlockHeader,
    header_pages: Vec<*mut u8>,
    memory_pools: Vec<*mut BlockHeader>,
}

impl MultiPoolTLSF {
    const HEADER_PAGE_SIZE: usize = 64;

    /// Allocates a block header from the header free list. The header is removed from the list and
    /// hence **must** be used or freed again. Otherwise memory will leak.
    unsafe fn allocate_header(&mut self) -> NonNull<BlockHeader> {
        if let Some(head) = self.header_free_list_head.as_mut() {
            self.header_free_list_head = head.next_free;

            head.next_free = null_mut();
            NonNull::new_unchecked(head)
        } else {
            // Need to allocate a new header page
            let page = std::alloc::alloc(Layout::new::<[BlockHeader; Self::HEADER_PAGE_SIZE]>());
            self.header_pages.push(page);

            let headers = page as *mut BlockHeader;

            // The first header we will return hence we dont need to insert it into the list
            *headers = BlockHeader::new_free_list(null_mut(), null_mut());

            let mut prev_free = null_mut();
            for i in 1..(Self::HEADER_PAGE_SIZE - 1) {
                let current = unsafe { headers.add(i) };
                let next_free = unsafe { current.add(1) };

                *current = BlockHeader::new_free_list(prev_free, next_free);

                prev_free = current;
            }

            *(headers.add(Self::HEADER_PAGE_SIZE - 1)) = BlockHeader::new_free_list(prev_free, null_mut());

            self.header_free_list_head = headers.add(1);
            NonNull::new_unchecked(headers)
        }
    }

    unsafe fn free_header(&mut self, mut header: NonNull<BlockHeader>) {
        header.as_mut().set_free_head(self.header_free_list_head);
        self.header_free_list_head = header.as_ptr();
    }
}

impl Drop for MultiPoolTLSF {
    fn drop(&mut self) {
        todo!()
    }
}

#[repr(C)]
struct BlockHeader {
    size: usize, // 16 Byte alignment guarantee -> 4 free bits to work with
    prev_physical: *mut BlockHeader,
    next_physical: *mut BlockHeader,
    prev_free: *mut BlockHeader,
    next_free: *mut BlockHeader,
}
assert_eq_size!(BlockHeader, [usize; 5]);

impl BlockHeader {
    const NULL_MASK: usize = 0b11;

    /// Constructs a new header that is part of the header free list. All fields will be initialized
    /// to 0/null except for `prev_free` and `next_free`.
    fn new_free_list(prev_free: *mut BlockHeader, next_free: *mut BlockHeader) -> Self {
        Self {
            size: 0,
            prev_physical: null_mut(),
            next_physical: null_mut(),
            prev_free,
            next_free
        }
    }

    fn get_size(&self) -> usize {
        self.size & !Self::NULL_MASK
    }

    /// Sets all fields such that this header becomes the new header free list head
    fn set_free_head(&mut self, next_free: *mut BlockHeader) {
        self.prev_free = null_mut();
        self.next_free = next_free;
    }

    /// Sets all fields such that this header becomes the head of a new fresh memory pool
    fn set_pool_head(&mut self, size: usize, prev_free: *mut BlockHeader, next_free: *mut BlockHeader) {
        assert_eq!(size & Self::NULL_MASK, 0usize);

        self.size = size;
        self.prev_physical = null_mut();
        self.next_physical = null_mut();
        self.prev_free = prev_free;
        self.next_free = next_free;
    }
}