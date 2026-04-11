use tetrisEngine::{Board, PieceKind};

use crate::data::BagSet;
use crate::movegen::{find_placements, simulate_lock_line_count};

pub fn count_spin_setups(board: &Board, piece: PieceKind, bag: BagSet) -> [u32; 4] {
    if !bag.contains(piece) {
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
    counts
}
