use crate::duration::parse_duration;

/// Compute next run timestamp (epoch seconds) from a duration string.
///
/// BUG: unchecked addition can overflow.
pub fn next_run_at(now_epoch: u64, every: &str) -> Option<u64> {
    let seconds = parse_duration(every)?;
    Some(now_epoch + seconds)
}

/// Return true if enough time has elapsed to run again.
pub fn should_run(last_run_epoch: u64, now_epoch: u64, every: &str) -> bool {
    match next_run_at(last_run_epoch, every) {
        Some(next) => now_epoch >= next,
        None => false,
    }
}
