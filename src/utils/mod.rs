mod external_guard;

pub use external_guard::ExternalGuard;
pub use external_guard::ExternallyGuarded;

macro_rules! define_counting_id_type {
    ($name:ident) => {
        #[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name {
            value: NonZeroU64,
        }

        impl $name {
            pub fn new() -> Self {
                use std::sync::atomic::{AtomicU64, Ordering};
                static NEXT_ID: AtomicU64 = AtomicU64::new(1);

                let value = match NonZeroU64::new(NEXT_ID.fetch_add(1, Ordering::Relaxed)) {
                    Some(value) => value,
                    // Scene ids may be used for correctness checks in unsafe code so we must not allow duplicates
                    None => std::process::abort(),
                };

                Self {
                    value,
                }
            }

            pub fn get_raw(&self) -> u64 {
                self.value.get()
            }

            pub fn get_nonzero(&self) -> NonZeroU64 {
                self.value
            }
        }
    };
}

pub(crate) use define_counting_id_type;