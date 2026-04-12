use tetrisEngine::{Board, PieceKind};

use crate::data::{BagSet, Placement};
use crate::movegen::{find_placements, simulate_lock_line_count};
use std::time::Instant;

pub fn has_any_spin_setup(placements: &[(Placement, u32)]) -> bool {
    placements.iter().any(|(placement, _)| placement.is_spin)
}

pub fn count_spin_setups(board: &Board, piece: PieceKind, bag: BagSet) -> [u32; 4] {
    let start = Instant::now();
    if !bag.contains(piece) {
        crate::profiling::record_slot(start.elapsed());
        return [0; 4];
    }
    let mut counts = [0u32; 4];
    for (placement, _) in find_placements(board, piece) {
        if placement.is_spin {
            let lines = simulate_lock_line_count(board, piece, &placement);
            let idx = lines as usize;
            if idx < 4 {
                counts[idx] = counts[idx].saturating_add(1);
            }
        }
    }
    crate::profiling::record_slot(start.elapsed());
    counts
}
