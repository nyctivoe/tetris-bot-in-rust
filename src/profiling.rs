use std::cell::Cell;
use std::time::Duration;

#[derive(Clone, Copy, Debug, Default)]
pub struct SearchProfile {
    pub movegen_calls: u64,
    pub movegen_nanos: u64,
    pub slot_calls: u64,
    pub slot_nanos: u64,
}

thread_local! {
    static SEARCH_PROFILE: Cell<SearchProfile> = Cell::new(SearchProfile::default());
}

pub fn clear() {
    SEARCH_PROFILE.with(|profile| profile.set(SearchProfile::default()));
}

pub fn take() -> SearchProfile {
    SEARCH_PROFILE.with(|profile| {
        let snapshot = profile.get();
        profile.set(SearchProfile::default());
        snapshot
    })
}

pub fn record_movegen(duration: Duration) {
    SEARCH_PROFILE.with(|profile| {
        let mut snapshot = profile.get();
        snapshot.movegen_calls += 1;
        snapshot.movegen_nanos = snapshot
            .movegen_nanos
            .saturating_add(duration.as_nanos() as u64);
        profile.set(snapshot);
    });
}

pub fn record_slot(duration: Duration) {
    SEARCH_PROFILE.with(|profile| {
        let mut snapshot = profile.get();
        snapshot.slot_calls += 1;
        snapshot.slot_nanos = snapshot
            .slot_nanos
            .saturating_add(duration.as_nanos() as u64);
        profile.set(snapshot);
    });
}
