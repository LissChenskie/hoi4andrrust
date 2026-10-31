//! Cadences are game-hour based, so results do not depend on FPS or device speed.
//! The intervals are provisional starting points; they must be re-tuned from
//! profiling data (see docs/ai-implementation-progress.md), not treated as final.
use super::state::AiProfile;

pub const HOURS_PER_DAY: u64 = 24;
/// How often the strategic profile (specialisation, posture) is re-evaluated.
pub const STRATEGIC_REVIEW_HOURS: u64 = 30 * HOURS_PER_DAY;

pub fn strategic_review_due(profile: &AiProfile, hour: u64) -> bool {
    match profile.last_strategic_review_hour {
        None => true,
        Some(last) => hour.saturating_sub(last) >= STRATEGIC_REVIEW_HOURS,
    }
}

/// A war starting or ending is a significant change: re-evaluate at once
/// instead of waiting for the periodic review.
pub fn posture_changed(profile: &AiProfile, at_war: bool) -> bool {
    use super::state::Posture::*;
    matches!((profile.posture, at_war), (Peace, true) | (War, false))
}
