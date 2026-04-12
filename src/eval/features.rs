use tetrisEngine::{Board, PieceKind, BOARD_HEIGHT, BOARD_WIDTH};

use crate::data::{BagSet, GameState, Placement, PlacementInfo};
use crate::eval::slots::count_spin_setups;

#[derive(Clone, Debug, Default)]
pub struct BoardFeatures {
    pub max_height: u32,
    pub height_above_10: u32,
    pub height_above_15: u32,
    pub holes: u32,
    pub cell_coveredness: u32,
    pub row_transitions: u32,
    pub col_transitions: u32,
    pub tetris_well_depth: u32,
    pub has_back_to_back: bool,
    pub b2b_chain: i32,
    pub surge_charge: i32,

    pub tslot: [u32; 4],
    pub jslot: [u32; 3],
    pub lslot: [u32; 3],
    pub sslot: [u32; 2],
    pub zslot: [u32; 2],
}

#[derive(Clone, Debug, Default)]
pub struct PlacementFeatures {
    pub placed_kind: Option<PieceKind>,
    pub lines_cleared: u32,
    pub is_spin: bool,
    pub is_mini: bool,
    pub is_back_to_back: bool,
    pub is_perfect_clear: bool,
    pub combo: u32,
    pub attack: i32,
    pub soft_drop_distance: u32,
    pub used_hold: bool,
    pub wasted_t: bool,
    pub wasted_spin_piece: bool,
    pub surge_send: i32,
}

pub fn extract_features(
    after_state: &GameState,
    info: &PlacementInfo,
    placement: Placement,
    soft_drop: u32,
    had_t_slot: bool,
    had_piece_spin_setup: bool,
) -> (BoardFeatures, PlacementFeatures) {
    let board_feats = extract_board_features(
        &after_state.board,
        after_state.b2b_chain,
        after_state.surge_charge,
        after_state.bag,
    );
    let placement_feats =
        extract_placement_features(info, placement, soft_drop, had_t_slot, had_piece_spin_setup);
    (board_feats, placement_feats)
}

fn column_heights(board: &Board) -> [u32; BOARD_WIDTH] {
    let mut heights = [0u32; BOARD_WIDTH];
    for col in 0..BOARD_WIDTH {
        for row in 0..BOARD_HEIGHT {
            if board[row * BOARD_WIDTH + col] != 0 {
                heights[col] = (BOARD_HEIGHT - row) as u32;
                break;
            }
        }
    }
    heights
}

fn extract_board_features(
    board: &Board,
    b2b_chain: i32,
    surge_charge: i32,
    bag: BagSet,
) -> BoardFeatures {
    let heights = column_heights(board);

    let max_height = *heights.iter().max().unwrap_or(&0);
    let height_above_10 = max_height.saturating_sub(10);
    let height_above_15 = max_height.saturating_sub(15);

    let mut holes = 0u32;
    let mut cell_coveredness = 0u32;
    for col in 0..BOARD_WIDTH {
        let h = heights[col];
        if h == 0 {
            continue;
        }
        let top_row = BOARD_HEIGHT - h as usize;
        for row in (top_row + 1)..BOARD_HEIGHT {
            if board[row * BOARD_WIDTH + col] == 0 {
                holes += 1;
                let depth = (row - top_row) as u32;
                cell_coveredness += depth;
            }
        }
    }

    let mut row_transitions = 0u32;
    for row in 0..BOARD_HEIGHT {
        let base = row * BOARD_WIDTH;
        let mut prev_filled = true;
        for col in 0..=BOARD_WIDTH {
            let filled = if col < BOARD_WIDTH {
                board[base + col] != 0
            } else {
                true
            };
            if filled != prev_filled {
                row_transitions += 1;
            }
            prev_filled = filled;
        }
    }

    let mut col_transitions = 0u32;
    for col in 0..BOARD_WIDTH {
        let h = heights[col];
        if h == 0 {
            continue;
        }
        let top_row = BOARD_HEIGHT - h as usize;
        let mut prev = true;
        for row in top_row..BOARD_HEIGHT {
            let filled = board[row * BOARD_WIDTH + col] != 0;
            if filled != prev {
                col_transitions += 1;
            }
            prev = filled;
        }
        if !prev {
            col_transitions += 1;
        }
    }

    let (tetris_well_col, _) = heights
        .iter()
        .enumerate()
        .min_by_key(|&(_, h)| h)
        .unwrap_or((0, &0));
    let mut tetris_well_depth = 0u32;
    'outer: for depth in 1..=max_height {
        let check_row = BOARD_HEIGHT - (max_height - depth + 1) as usize;
        if check_row >= BOARD_HEIGHT {
            break;
        }
        for col in 0..BOARD_WIDTH {
            if col == tetris_well_col {
                continue;
            }
            if board[check_row * BOARD_WIDTH + col] == 0 {
                break 'outer;
            }
        }
        tetris_well_depth = depth;
    }

    let tslot = count_spin_setups(board, PieceKind::T, bag);
    let jslot = count_spin_setups(board, PieceKind::J, bag);
    let lslot = count_spin_setups(board, PieceKind::L, bag);
    let sslot = count_spin_setups(board, PieceKind::S, bag);
    let zslot = count_spin_setups(board, PieceKind::Z, bag);

    BoardFeatures {
        max_height,
        height_above_10,
        height_above_15,
        holes,
        cell_coveredness,
        row_transitions,
        col_transitions,
        tetris_well_depth,
        has_back_to_back: b2b_chain > 0,
        b2b_chain,
        surge_charge,
        tslot: [tslot[0], tslot[1], tslot[2], tslot[3]],
        jslot: [jslot[0], jslot[1], jslot[2]],
        lslot: [lslot[0], lslot[1], lslot[2]],
        sslot: [sslot[0], sslot[1]],
        zslot: [zslot[0], zslot[1]],
    }
}

fn extract_placement_features(
    info: &PlacementInfo,
    placement: Placement,
    soft_drop: u32,
    had_t_slot: bool,
    had_piece_spin_setup: bool,
) -> PlacementFeatures {
    let wasted_t = placement.kind == PieceKind::T && !info.is_spin && had_t_slot;
    let wasted_spin_piece = matches!(
        placement.kind,
        PieceKind::J | PieceKind::L | PieceKind::S | PieceKind::Z | PieceKind::I
    ) && !info.is_spin
        && had_piece_spin_setup;

    PlacementFeatures {
        placed_kind: info.placed_kind,
        lines_cleared: info.lines_cleared,
        is_spin: info.is_spin,
        is_mini: info.is_mini,
        is_back_to_back: info.b2b_bonus > 0,
        is_perfect_clear: info.perfect_clear,
        combo: info.combo as u32,
        attack: info.attack,
        soft_drop_distance: soft_drop,
        used_hold: info.used_hold,
        wasted_t,
        wasted_spin_piece,
        surge_send: info.surge_send,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_board_has_zero_features() {
        let board = [0i8; BOARD_WIDTH * BOARD_HEIGHT];
        let feats = extract_board_features(&board, 0, 0, BagSet::full());
        assert_eq!(feats.max_height, 0);
        assert_eq!(feats.holes, 0);
        assert_eq!(feats.cell_coveredness, 0);
        assert_eq!(feats.tetris_well_depth, 0);
    }

    #[test]
    fn full_bottom_row_gives_height_one() {
        let mut board = [0i8; BOARD_WIDTH * BOARD_HEIGHT];
        for x in 0..BOARD_WIDTH {
            board[39 * BOARD_WIDTH + x] = 1;
        }
        let feats = extract_board_features(&board, 0, 0, BagSet::full());
        assert_eq!(feats.max_height, 1);
    }

    #[test]
    fn hole_detection() {
        let mut board = [0i8; BOARD_WIDTH * BOARD_HEIGHT];
        board[38 * BOARD_WIDTH + 0] = 1;
        board[39 * BOARD_WIDTH + 0] = 0;
        board[39 * BOARD_WIDTH + 1] = 1;
        let feats = extract_board_features(&board, 0, 0, BagSet::full());
        assert!(feats.holes > 0);
    }

    #[test]
    fn placement_features_mark_wasted_t_from_precomputed_slot() {
        let info = PlacementInfo {
            placed_kind: Some(PieceKind::T),
            ..PlacementInfo::default()
        };
        let placement = Placement {
            x: 4,
            y: 18,
            rotation: 0,
            kind: PieceKind::T,
            last_was_rot: false,
            last_rot_dir: None,
            last_kick_idx: None,
            is_spin: false,
            is_mini: false,
        };

        let feats = extract_placement_features(&info, placement, 0, true, true);

        assert!(feats.wasted_t);
        assert!(!feats.wasted_spin_piece);
    }

    #[test]
    fn placement_features_mark_wasted_all_spin_piece_from_precomputed_slot() {
        let info = PlacementInfo {
            placed_kind: Some(PieceKind::J),
            ..PlacementInfo::default()
        };
        let placement = Placement {
            x: 4,
            y: 18,
            rotation: 0,
            kind: PieceKind::J,
            last_was_rot: false,
            last_rot_dir: None,
            last_kick_idx: None,
            is_spin: false,
            is_mini: false,
        };

        let feats = extract_placement_features(&info, placement, 0, false, true);

        assert!(!feats.wasted_t);
        assert!(feats.wasted_spin_piece);
    }
}
