use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};
use tetrisEngine::{
    base_attack_for_clear, board_index, classify_clear, combo_after_clear, combo_attack_down,
    compute_blocks, piece_id, update_b2b_state, B2BMode, Board, Piece, PieceKind, SpinResult,
    BOARD_HEIGHT, BOARD_WIDTH,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BagSet(pub u8);

impl BagSet {
    pub const FULL: BagSet = BagSet(0b0111_1111);

    #[inline]
    pub fn full() -> Self {
        Self::FULL
    }

    #[inline]
    pub fn empty() -> Self {
        BagSet(0)
    }

    #[inline]
    pub fn single(kind: PieceKind) -> Self {
        BagSet(1 << kind as u8)
    }

    #[inline]
    pub fn contains(self, kind: PieceKind) -> bool {
        self.0 & (1 << kind as u8) != 0
    }

    #[inline]
    pub fn remove(&mut self, kind: PieceKind) {
        self.0 &= !(1 << kind as u8);
    }

    #[inline]
    pub fn insert(&mut self, kind: PieceKind) {
        self.0 |= 1 << kind as u8;
    }

    #[inline]
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    #[inline]
    pub fn bits(self) -> u8 {
        self.0
    }

    #[inline]
    pub fn after_consuming(self, kind: PieceKind) -> Self {
        let mut next = self;
        next.remove(kind);
        if next.is_empty() {
            Self::full()
        } else {
            next
        }
    }

    pub fn iter(self) -> impl Iterator<Item = PieceKind> {
        let bits = self.0;
        static KINDS: [PieceKind; 7] = [
            PieceKind::I,
            PieceKind::O,
            PieceKind::T,
            PieceKind::S,
            PieceKind::Z,
            PieceKind::J,
            PieceKind::L,
        ];
        KINDS
            .iter()
            .copied()
            .filter(move |&k| bits & (1 << k as u8) != 0)
    }

    #[inline]
    pub fn len(self) -> usize {
        self.0.count_ones() as usize
    }
}

impl Hash for BagSet {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct Placement {
    pub x: i16,
    pub y: i16,
    pub rotation: u8,
    pub kind: PieceKind,
    pub last_was_rot: bool,
    pub last_rot_dir: Option<i8>,
    pub last_kick_idx: Option<u8>,
    pub is_spin: bool,
    pub is_mini: bool,
}

impl Placement {
    pub fn to_piece(self) -> Piece {
        Piece {
            kind: self.kind,
            rotation: self.rotation % 4,
            position: (self.x, self.y),
            last_action_was_rotation: self.last_was_rot,
            last_rotation_dir: self.last_rot_dir,
            last_kick_index: self.last_kick_idx,
        }
    }

    pub fn blocks(self) -> [(i16, i16); 4] {
        let piece = self.to_piece();
        compute_blocks(&piece, None, None)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum SearchActionKind {
    PlayCurrent,
    UseHold,
    HoldEmpty,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct SearchAction {
    pub incoming_kind: PieceKind,
    pub placement: Placement,
    pub kind: SearchActionKind,
}

impl SearchAction {
    pub fn play_current(incoming_kind: PieceKind, placement: Placement) -> Self {
        debug_assert_eq!(placement.kind, incoming_kind);
        Self {
            incoming_kind,
            placement,
            kind: SearchActionKind::PlayCurrent,
        }
    }

    pub fn use_hold(incoming_kind: PieceKind, placement: Placement) -> Self {
        debug_assert_ne!(placement.kind, incoming_kind);
        Self {
            incoming_kind,
            placement,
            kind: SearchActionKind::UseHold,
        }
    }

    pub fn hold_empty(incoming_kind: PieceKind, placement: Placement) -> Self {
        Self {
            incoming_kind,
            placement,
            kind: SearchActionKind::HoldEmpty,
        }
    }

    pub fn played_kind(self) -> PieceKind {
        self.placement.kind
    }

    pub fn used_hold(self) -> bool {
        self.kind != SearchActionKind::PlayCurrent
    }

    pub fn consumes_next_piece(self) -> bool {
        self.kind == SearchActionKind::HoldEmpty
    }

    pub fn queue_consumption(self) -> usize {
        if self.consumes_next_piece() {
            2
        } else {
            1
        }
    }

    pub fn from_placement(
        state: &GameState,
        incoming_kind: PieceKind,
        placement: Placement,
    ) -> Self {
        if placement.kind == incoming_kind {
            return Self::play_current(incoming_kind, placement);
        }

        match state.reserve {
            Some(reserve) if reserve == placement.kind => Self::use_hold(incoming_kind, placement),
            None => Self::hold_empty(incoming_kind, placement),
            Some(reserve) => {
                debug_assert_eq!(reserve, placement.kind);
                Self::use_hold(incoming_kind, placement)
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PlacementInfo {
    pub lines_cleared: u32,
    pub attack: i32,
    pub b2b_chain: i32,
    pub b2b_bonus: i32,
    pub surge_charge: i32,
    pub surge_send: i32,
    pub combo: i32,
    pub combo_active: bool,
    pub perfect_clear: bool,
    pub is_spin: bool,
    pub is_mini: bool,
    pub is_difficult: bool,
    pub base_attack: i32,
    pub combo_attack: i32,
    pub placed_kind: Option<PieceKind>,
    pub used_hold: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct GameState {
    pub board: Board,
    pub bag: BagSet,
    pub reserve: Option<PieceKind>,
    pub b2b_chain: i32,
    pub surge_charge: i32,
    pub combo: i32,
    pub combo_active: bool,
    pub b2b_mode: B2BMode,
}

impl Hash for GameState {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.board.hash(state);
        self.bag.hash(state);
        self.reserve.hash(state);
        self.b2b_chain.hash(state);
        self.surge_charge.hash(state);
        self.combo.hash(state);
        self.combo_active.hash(state);
        match self.b2b_mode {
            B2BMode::Surge => 0u8,
            B2BMode::Chaining => 1u8,
        }
        .hash(state);
    }
}

impl PartialEq for GameState {
    fn eq(&self, other: &Self) -> bool {
        self.board == other.board
            && self.bag == other.bag
            && self.reserve == other.reserve
            && self.b2b_chain == other.b2b_chain
            && self.surge_charge == other.surge_charge
            && self.combo == other.combo
            && self.combo_active == other.combo_active
            && self.b2b_mode == other.b2b_mode
    }
}

impl Eq for GameState {}

impl GameState {
    pub fn empty() -> Self {
        Self {
            board: [0; BOARD_WIDTH * BOARD_HEIGHT],
            bag: BagSet::full(),
            reserve: None,
            b2b_chain: 0,
            surge_charge: 0,
            combo: 0,
            combo_active: false,
            b2b_mode: B2BMode::Surge,
        }
    }

    pub fn advance(&mut self, kind: PieceKind, placement: Placement) -> PlacementInfo {
        self.advance_action(SearchAction::from_placement(self, kind, placement))
    }

    pub fn advance_action(&mut self, action: SearchAction) -> PlacementInfo {
        self.advance_internal(action)
    }

    fn advance_internal(&mut self, action: SearchAction) -> PlacementInfo {
        let incoming_kind = action.incoming_kind;
        let placement = action.placement;
        let used_hold = action.used_hold();
        let placed_kind = placement.kind;
        debug_assert!(used_hold || placed_kind == incoming_kind);
        debug_assert!(
            action.kind != SearchActionKind::UseHold || self.reserve == Some(placed_kind)
        );
        debug_assert!(!action.consumes_next_piece() || self.reserve.is_none());

        let id = piece_id(placed_kind);
        for (x, y) in placement.blocks() {
            if let Some(idx) = board_index(x, y) {
                self.board[idx] = id;
            }
        }

        let (board_after, lines_cleared) = Self::clear_lines(&self.board);
        self.board = board_after;

        let perfect_clear = self.board.iter().all(|&cell| cell == 0);

        let spin_result = Self::spin_result_for_placement(placement);
        let classification = classify_clear(lines_cleared, spin_result.as_ref(), perfect_clear);
        let (base_attack, _pc) =
            base_attack_for_clear(lines_cleared, spin_result.as_ref(), &self.board);

        let combo_update = combo_after_clear(lines_cleared, self.combo, self.combo_active);
        self.combo = combo_update.combo;
        self.combo_active = combo_update.combo_active;

        let b2b_update = update_b2b_state(
            self.b2b_mode,
            lines_cleared,
            classification.is_difficult,
            self.b2b_chain,
            self.surge_charge,
        );
        self.b2b_chain = b2b_update.b2b_chain;
        self.surge_charge = b2b_update.surge_charge;

        let combo_atk = combo_attack_down(base_attack, self.combo);
        let attack_total = combo_atk + b2b_update.b2b_bonus + b2b_update.surge_send;

        self.consume_piece_from_bag(incoming_kind);
        if action.consumes_next_piece() {
            self.consume_piece_from_bag(placed_kind);
        }
        if used_hold {
            self.reserve = Some(incoming_kind);
        }

        PlacementInfo {
            lines_cleared: lines_cleared as u32,
            attack: attack_total,
            b2b_chain: self.b2b_chain,
            b2b_bonus: b2b_update.b2b_bonus,
            surge_charge: self.surge_charge,
            surge_send: b2b_update.surge_send,
            combo: self.combo,
            combo_active: self.combo_active,
            perfect_clear,
            is_spin: classification.is_spin,
            is_mini: classification.is_mini,
            is_difficult: classification.is_difficult,
            base_attack,
            combo_attack: combo_atk,
            placed_kind: Some(placed_kind),
            used_hold,
        }
    }

    pub fn clear_lines(board: &Board) -> (Board, i32) {
        let mut cleared: i32 = 0;
        let mut compacted = [0i8; BOARD_WIDTH * BOARD_HEIGHT];
        let mut write_row = BOARD_HEIGHT;

        for read_row in (0..BOARD_HEIGHT).rev() {
            let start = read_row * BOARD_WIDTH;
            let end = start + BOARD_WIDTH;
            let is_full = board[start..end].iter().all(|&cell| cell != 0);
            if is_full {
                cleared += 1;
                continue;
            }
            write_row -= 1;
            let write_start = write_row * BOARD_WIDTH;
            compacted[write_start..write_start + BOARD_WIDTH].copy_from_slice(&board[start..end]);
        }
        (compacted, cleared)
    }

    fn spin_result_for_placement(placement: Placement) -> Option<SpinResult> {
        if !placement.is_spin {
            return None;
        }

        let is_180 = placement.last_rot_dir.unwrap_or(0).abs() == 2;
        let description = match placement.kind {
            PieceKind::T => format!(
                "{}T-Spin{}",
                if is_180 { "180 " } else { "" },
                if placement.is_mini { " Mini" } else { "" }
            ),
            _ => format!("{:?}-Spin Mini", placement.kind),
        };

        Some(SpinResult {
            piece: placement.kind,
            spin_type: if placement.kind == PieceKind::T {
                "t-spin"
            } else {
                "spin"
            },
            is_mini: placement.is_mini,
            is_180,
            kick_index: placement.last_kick_idx,
            rotation_dir: placement.last_rot_dir,
            corners: None,
            front_corners: None,
            description,
        })
    }

    fn consume_piece_from_bag(&mut self, kind: PieceKind) {
        self.bag = self.bag.after_consuming(kind);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    fn fill_row(board: &mut Board, row: usize) {
        let start = row * BOARD_WIDTH;
        for i in start..start + BOARD_WIDTH {
            board[i] = 1;
        }
    }

    #[test]
    fn bag_set_full_contains_all_pieces() {
        let bag = BagSet::full();
        assert!(bag.contains(PieceKind::I));
        assert!(bag.contains(PieceKind::O));
        assert!(bag.contains(PieceKind::T));
        assert!(bag.contains(PieceKind::S));
        assert!(bag.contains(PieceKind::Z));
        assert!(bag.contains(PieceKind::J));
        assert!(bag.contains(PieceKind::L));
        assert_eq!(bag.iter().count(), 7);
    }

    #[test]
    fn bag_set_remove_and_refill() {
        let mut bag = BagSet::full();
        bag.remove(PieceKind::T);
        assert!(!bag.contains(PieceKind::T));
        assert_eq!(bag.iter().count(), 6);

        bag.remove(PieceKind::I);
        bag.remove(PieceKind::O);
        bag.remove(PieceKind::S);
        bag.remove(PieceKind::Z);
        bag.remove(PieceKind::J);
        bag.remove(PieceKind::L);
        assert!(bag.is_empty());

        let mut bag = BagSet::full();
        for kind in [
            PieceKind::I,
            PieceKind::O,
            PieceKind::T,
            PieceKind::S,
            PieceKind::Z,
            PieceKind::J,
        ] {
            bag.remove(kind);
        }
        assert!(!bag.is_empty());
        assert!(bag.contains(PieceKind::L));
        bag.remove(PieceKind::L);
        assert!(bag.is_empty());
    }

    #[test]
    fn bag_set_iter_returns_correct_pieces() {
        let mut bag = BagSet::full();
        bag.remove(PieceKind::O);
        bag.remove(PieceKind::Z);
        let pieces: Vec<PieceKind> = bag.iter().collect();
        assert_eq!(
            pieces,
            vec![
                PieceKind::I,
                PieceKind::T,
                PieceKind::S,
                PieceKind::J,
                PieceKind::L
            ]
        );
    }

    #[test]
    fn game_state_hash_is_stable() {
        let state = GameState::empty();
        let mut hasher1 = DefaultHasher::new();
        state.hash(&mut hasher1);
        let h1 = hasher1.finish();

        let mut hasher2 = DefaultHasher::new();
        state.hash(&mut hasher2);
        let h2 = hasher2.finish();

        assert_eq!(h1, h2);
    }

    #[test]
    fn game_state_equal_states_hash_identically() {
        let s1 = GameState::empty();
        let s2 = GameState::empty();
        assert_eq!(s1, s2);

        let mut h1 = DefaultHasher::new();
        s1.hash(&mut h1);
        let mut h2 = DefaultHasher::new();
        s2.hash(&mut h2);
        assert_eq!(h1.finish(), h2.finish());
    }

    #[test]
    fn placement_blocks_match_engine_compute_blocks() {
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
        let blocks = placement.blocks();
        let piece = Piece::new(PieceKind::T, 0, (4, 18));
        let engine_blocks = compute_blocks(&piece, None, None);
        assert_eq!(blocks, engine_blocks);
    }

    #[test]
    fn advance_places_piece_and_clears_lines() {
        let mut state = GameState::empty();

        fill_row(&mut state.board, 39);
        fill_row(&mut state.board, 38);
        fill_row(&mut state.board, 37);
        for x in 0..3 {
            state.board[36 * BOARD_WIDTH + x] = 1;
        }
        for x in 7..10 {
            state.board[36 * BOARD_WIDTH + x] = 1;
        }

        let placement = Placement {
            x: 3,
            y: 35,
            rotation: 0,
            kind: PieceKind::I,
            last_was_rot: false,
            last_rot_dir: None,
            last_kick_idx: None,
            is_spin: false,
            is_mini: false,
        };
        let info = state.advance(PieceKind::I, placement);
        assert_eq!(info.lines_cleared, 4);
        assert!(!state.board.iter().any(|&c| c != 0));
    }

    #[test]
    fn advance_updates_b2b_on_tetris() {
        let mut state = GameState::empty();
        fill_row(&mut state.board, 39);
        fill_row(&mut state.board, 38);
        fill_row(&mut state.board, 37);
        for x in 0..3 {
            state.board[36 * BOARD_WIDTH + x] = 1;
        }
        for x in 7..10 {
            state.board[36 * BOARD_WIDTH + x] = 1;
        }

        let placement = Placement {
            x: 3,
            y: 35,
            rotation: 0,
            kind: PieceKind::I,
            last_was_rot: false,
            last_rot_dir: None,
            last_kick_idx: None,
            is_spin: false,
            is_mini: false,
        };
        let info = state.advance(PieceKind::I, placement);
        assert_eq!(info.lines_cleared, 4);
        assert!(info.is_difficult);
        assert_eq!(info.b2b_chain, 1);
    }

    #[test]
    fn advance_updates_combo() {
        let mut state = GameState::empty();
        fill_row(&mut state.board, 39);

        let placement = Placement {
            x: 0,
            y: 38,
            rotation: 0,
            kind: PieceKind::I,
            last_was_rot: false,
            last_rot_dir: None,
            last_kick_idx: None,
            is_spin: false,
            is_mini: false,
        };
        let info = state.advance(PieceKind::I, placement);
        assert_eq!(info.lines_cleared, 1);
        assert!(info.combo_active);
        assert_eq!(info.combo, 0);

        fill_row(&mut state.board, 39);
        let placement2 = Placement {
            x: 0,
            y: 38,
            rotation: 0,
            kind: PieceKind::I,
            last_was_rot: false,
            last_rot_dir: None,
            last_kick_idx: None,
            is_spin: false,
            is_mini: false,
        };
        let info2 = state.advance(PieceKind::I, placement2);
        assert_eq!(info2.combo, 1);
    }

    #[test]
    fn advance_detects_perfect_clear() {
        let mut state = GameState::empty();
        for x in 0..10 {
            if x != 1 && x != 2 {
                state.board[38 * BOARD_WIDTH + x] = 1;
                state.board[39 * BOARD_WIDTH + x] = 1;
            }
        }

        let placement = Placement {
            x: 0,
            y: 37,
            rotation: 0,
            kind: PieceKind::O,
            last_was_rot: false,
            last_rot_dir: None,
            last_kick_idx: None,
            is_spin: false,
            is_mini: false,
        };
        let info = state.advance(PieceKind::O, placement);
        assert_eq!(info.lines_cleared, 2);
        assert!(info.perfect_clear);
    }

    #[test]
    fn advance_tspin_attack() {
        let mut state = GameState::empty();

        let placement = Placement {
            x: 4,
            y: 38,
            rotation: 0,
            kind: PieceKind::T,
            last_was_rot: true,
            last_rot_dir: Some(1),
            last_kick_idx: Some(0),
            is_spin: true,
            is_mini: false,
        };

        let blocks = placement.blocks();
        let target_row = blocks.iter().map(|&(_, y)| y).max().unwrap();
        for x in 0..BOARD_WIDTH as i16 {
            if !blocks.iter().any(|&(bx, by)| bx == x && by == target_row) {
                state.board[board_index(x, target_row).unwrap()] = 1;
            }
        }

        let info = state.advance(PieceKind::T, placement);
        assert_eq!(info.lines_cleared, 1);
        assert_eq!(info.base_attack, 2);
        assert!(info.is_spin);
        assert!(!info.is_mini);
    }

    #[test]
    fn advance_removes_piece_from_bag_and_refills() {
        let mut state = GameState::empty();
        assert!(state.bag.contains(PieceKind::T));

        let placement = Placement {
            x: 3,
            y: 18,
            rotation: 0,
            kind: PieceKind::T,
            last_was_rot: false,
            last_rot_dir: None,
            last_kick_idx: None,
            is_spin: false,
            is_mini: false,
        };
        state.advance(PieceKind::T, placement);
        assert!(!state.bag.contains(PieceKind::T));
        assert_eq!(state.bag.iter().count(), 6);

        for kind in [
            PieceKind::I,
            PieceKind::O,
            PieceKind::S,
            PieceKind::Z,
            PieceKind::J,
            PieceKind::L,
        ] {
            let p = Placement {
                x: 3,
                y: 18,
                rotation: 0,
                kind,
                last_was_rot: false,
                last_rot_dir: None,
                last_kick_idx: None,
                is_spin: false,
                is_mini: false,
            };
            state.advance(kind, p);
        }
        assert!(!state.bag.is_empty());
        assert_eq!(state.bag.iter().count(), 7);
    }

    #[test]
    fn advance_tracks_hold_swap_and_placed_piece() {
        let mut state = GameState::empty();
        state.reserve = Some(PieceKind::O);
        state.bag.remove(PieceKind::O);

        let placement = Placement {
            x: 0,
            y: 37,
            rotation: 0,
            kind: PieceKind::O,
            last_was_rot: false,
            last_rot_dir: None,
            last_kick_idx: None,
            is_spin: false,
            is_mini: false,
        };

        let info = state.advance(PieceKind::T, placement);
        assert_eq!(info.placed_kind, Some(PieceKind::O));
        assert!(info.used_hold);
        assert_eq!(state.reserve, Some(PieceKind::T));
        assert!(!state.bag.contains(PieceKind::T));
    }

    #[test]
    fn advance_tracks_empty_hold_and_consumes_both_queue_pieces() {
        let mut state = GameState::empty();

        let placement = Placement {
            x: 0,
            y: 37,
            rotation: 0,
            kind: PieceKind::O,
            last_was_rot: false,
            last_rot_dir: None,
            last_kick_idx: None,
            is_spin: false,
            is_mini: false,
        };

        let info = state.advance(PieceKind::T, placement);
        assert_eq!(info.placed_kind, Some(PieceKind::O));
        assert!(info.used_hold);
        assert_eq!(state.reserve, Some(PieceKind::T));
        assert!(!state.bag.contains(PieceKind::T));
        assert!(!state.bag.contains(PieceKind::O));
    }

    #[test]
    fn advance_empty_hold_refills_when_current_piece_finishes_bag() {
        let mut state = GameState::empty();
        state.bag = BagSet::single(PieceKind::T);

        let placement = Placement {
            x: 0,
            y: 37,
            rotation: 0,
            kind: PieceKind::O,
            last_was_rot: false,
            last_rot_dir: None,
            last_kick_idx: None,
            is_spin: false,
            is_mini: false,
        };

        let info = state.advance(PieceKind::T, placement);
        assert_eq!(info.placed_kind, Some(PieceKind::O));
        assert!(info.used_hold);
        assert_eq!(state.reserve, Some(PieceKind::T));
        assert!(state.bag.contains(PieceKind::T));
        assert!(!state.bag.contains(PieceKind::O));
        assert_eq!(state.bag.iter().count(), 6);
    }

    #[test]
    fn game_state_equality_includes_b2b_mode() {
        let state = GameState::empty();
        let mut other = state;
        other.b2b_mode = B2BMode::Chaining;

        assert_ne!(state, other);
    }

    #[test]
    fn clear_lines_logic_matches_engine() {
        let mut board: Board = [0; BOARD_WIDTH * BOARD_HEIGHT];
        fill_row(&mut board, 39);
        fill_row(&mut board, 38);
        board[37 * BOARD_WIDTH + 0] = 1;
        board[37 * BOARD_WIDTH + 1] = 1;

        let (cleared_board, count) = GameState::clear_lines(&board);
        assert_eq!(count, 2);
        assert_eq!(cleared_board[39 * BOARD_WIDTH + 0], 1);
        assert_eq!(cleared_board[39 * BOARD_WIDTH + 1], 1);
        assert!(cleared_board[39 * BOARD_WIDTH + 2..40 * BOARD_WIDTH]
            .iter()
            .all(|&c| c == 0));
    }

    #[test]
    fn empty_board_clear_lines_returns_zero() {
        let board: Board = [0; BOARD_WIDTH * BOARD_HEIGHT];
        let (_, count) = GameState::clear_lines(&board);
        assert_eq!(count, 0);
    }

    #[test]
    fn placement_info_defaults_to_zero() {
        let info = PlacementInfo::default();
        assert_eq!(info.lines_cleared, 0);
        assert_eq!(info.attack, 0);
        assert_eq!(info.b2b_chain, 0);
        assert_eq!(info.combo, 0);
        assert!(!info.combo_active);
        assert!(!info.perfect_clear);
        assert!(!info.is_spin);
        assert!(!info.is_mini);
    }
}
