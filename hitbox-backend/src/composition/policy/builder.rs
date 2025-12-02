//! CompositionPolicy builder for configuring all three policies together.

use super::{
    CompositionReadPolicy, CompositionWritePolicy, OptimisticParallelWritePolicy, RefillPolicy,
    SequentialReadPolicy,
};

/// Bundle of read, write, and refill policies for CompositionBackend.
///
/// This struct provides a builder pattern for configuring all three policy types
/// together, making it easy to create and reuse policy configurations.
///
/// # Example
///
/// ```ignore
/// use hitbox_backend::composition::{CompositionPolicy, CompositionBackend};
/// use hitbox_backend::composition::policy::{RaceReadPolicy, SequentialWritePolicy, RefillPolicy};
///
/// let policy = CompositionPolicy::new()
///     .read(RaceReadPolicy::new())
///     .write(SequentialWritePolicy::new())
///     .refill(RefillPolicy::Always);
///
/// let backend = CompositionBackend::new(l1, l2)
///     .with_policy(policy);
/// ```
#[derive(Debug, Clone)]
pub struct CompositionPolicy<R = SequentialReadPolicy, W = OptimisticParallelWritePolicy>
where
    R: CompositionReadPolicy,
    W: CompositionWritePolicy,
{
    /// Read policy
    pub(crate) read: R,
    /// Write policy
    pub(crate) write: W,
    /// Refill policy
    pub(crate) refill: RefillPolicy,
}

impl CompositionPolicy<SequentialReadPolicy, OptimisticParallelWritePolicy> {
    /// Create a new policy bundle with default policies.
    ///
    /// Default policies:
    /// - Read: `SequentialReadPolicy` (try L1 first, then L2)
    /// - Write: `OptimisticParallelWritePolicy` (write to both, succeed if ≥1 succeeds)
    /// - Refill: `RefillPolicy::Never` (do not populate L1 after L2 hit)
    pub fn new() -> Self {
        Self {
            read: SequentialReadPolicy::new(),
            write: OptimisticParallelWritePolicy::new(),
            refill: RefillPolicy::default(),
        }
    }
}

impl Default for CompositionPolicy<SequentialReadPolicy, OptimisticParallelWritePolicy> {
    fn default() -> Self {
        Self::new()
    }
}

impl<R, W> CompositionPolicy<R, W>
where
    R: CompositionReadPolicy,
    W: CompositionWritePolicy,
{
    /// Set the read policy (builder pattern).
    ///
    /// # Example
    /// ```ignore
    /// use hitbox_backend::composition::CompositionPolicy;
    /// use hitbox_backend::composition::policy::RaceReadPolicy;
    ///
    /// let policy = CompositionPolicy::new()
    ///     .read(RaceReadPolicy::new());
    /// ```
    pub fn read<NewR: CompositionReadPolicy>(self, read: NewR) -> CompositionPolicy<NewR, W> {
        CompositionPolicy {
            read,
            write: self.write,
            refill: self.refill,
        }
    }

    /// Set the write policy (builder pattern).
    ///
    /// # Example
    /// ```ignore
    /// use hitbox_backend::composition::CompositionPolicy;
    /// use hitbox_backend::composition::policy::SequentialWritePolicy;
    ///
    /// let policy = CompositionPolicy::new()
    ///     .write(SequentialWritePolicy::new());
    /// ```
    pub fn write<NewW: CompositionWritePolicy>(self, write: NewW) -> CompositionPolicy<R, NewW> {
        CompositionPolicy {
            read: self.read,
            write,
            refill: self.refill,
        }
    }

    /// Set the refill policy (builder pattern).
    ///
    /// # Example
    /// ```ignore
    /// use hitbox_backend::composition::CompositionPolicy;
    /// use hitbox_backend::composition::policy::RefillPolicy;
    ///
    /// let policy = CompositionPolicy::new()
    ///     .refill(RefillPolicy::Always);
    /// ```
    pub fn refill(self, refill: RefillPolicy) -> CompositionPolicy<R, W> {
        CompositionPolicy {
            read: self.read,
            write: self.write,
            refill,
        }
    }

    /// Get a reference to the read policy.
    pub fn read_policy(&self) -> &R {
        &self.read
    }

    /// Get a reference to the write policy.
    pub fn write_policy(&self) -> &W {
        &self.write
    }

    /// Get a reference to the refill policy.
    pub fn refill_policy(&self) -> &RefillPolicy {
        &self.refill
    }
}
