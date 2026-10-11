//! Pure fencing checks shared by every SQL claim mutation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PolicyError {
    Stale,
    Exhausted,
}
pub(crate) fn require_live(
    actual_owner: Option<[u8; 16]>,
    actual_epoch: u64,
    until: u64,
    owner: [u8; 16],
    epoch: u64,
    now: u64,
) -> Result<(), PolicyError> {
    if epoch == 0 || actual_owner != Some(owner) || actual_epoch != epoch || until <= now {
        Err(PolicyError::Stale)
    } else {
        Ok(())
    }
}
pub(crate) fn next_epoch(epoch: u64) -> Result<u64, PolicyError> {
    epoch.checked_add(1).ok_or(PolicyError::Exhausted)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_current_live_credential_only() {
        assert_eq!(require_live(Some([1; 16]), 3, 101, [1; 16], 3, 100), Ok(()));
        for (owner, epoch, expiry, supplied_owner, supplied_epoch, now) in [
            (None, 3, 101, [1; 16], 3, 100),
            (Some([2; 16]), 3, 101, [1; 16], 3, 100),
            (Some([1; 16]), 4, 101, [1; 16], 3, 100),
            (Some([1; 16]), 3, 100, [1; 16], 3, 100),
            (Some([1; 16]), 3, 99, [1; 16], 3, 100),
            (Some([1; 16]), 0, 101, [1; 16], 0, 100),
        ] {
            assert_eq!(
                require_live(owner, epoch, expiry, supplied_owner, supplied_epoch, now),
                Err(PolicyError::Stale)
            );
        }
    }
    #[test]
    fn epoch_never_wraps() {
        assert_eq!(next_epoch(0), Ok(1));
        assert_eq!(next_epoch(u64::MAX), Err(PolicyError::Exhausted));
    }
}
