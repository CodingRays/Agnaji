mod external_guard;

pub use external_guard::ExternalGuard;
pub use external_guard::ExternallyGuarded;

macro_rules! define_counting_id_type {
    ($v:vis, $name:ident) => {
        #[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        $v struct $name {
            value: ::std::num::NonZeroU64,
        }

        impl $name {
            $v fn new() -> Self {
                use std::sync::atomic::{AtomicU64, Ordering};
                static NEXT_ID: AtomicU64 = AtomicU64::new(1);

                let value = match ::std::num::NonZeroU64::new(NEXT_ID.fetch_add(1, Ordering::Relaxed)) {
                    Some(value) => value,
                    // Scene ids may be used for correctness checks in unsafe code so we must not allow duplicates
                    None => ::std::process::abort(),
                };

                Self {
                    value,
                }
            }

            $v fn get_raw(&self) -> u64 {
                self.value.get()
            }

            $v fn get_nonzero(&self) -> ::std::num::NonZeroU64 {
                self.value
            }
        }

        impl ::std::fmt::Debug for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.debug_tuple(stringify!($name)).field(&self.value.get()).finish()
            }
        }
    };
}

pub(crate) use define_counting_id_type;