//! Refill policies for controlling L1 population after L2 hits.

/// Policy for controlling L1 refill after L2 hits.
///
/// When a read misses L1 but hits L2, the refill policy determines whether
/// to populate L1 with the value from L2.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RefillPolicy {
    /// Always populate L1 after L2 hit (classic cache hierarchy behavior).
    Always,
    /// Never populate L1 from L2 hits (L1 is write-only).
    #[default]
    Never,
}
