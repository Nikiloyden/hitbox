//! Tests for composition RefillPolicy enum.

use hitbox_backend::composition::policy::RefillPolicy;

#[test]
fn test_refill_policy_default_is_never() {
    let policy = RefillPolicy::default();
    assert_eq!(policy, RefillPolicy::Never);
}

#[test]
fn test_refill_policy_always_variant() {
    let policy = RefillPolicy::Always;
    assert_eq!(policy, RefillPolicy::Always);
}

#[test]
fn test_refill_policy_never_variant() {
    let policy = RefillPolicy::Never;
    assert_eq!(policy, RefillPolicy::Never);
}
