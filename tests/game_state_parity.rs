use tetrisBot::data::{BagSet, GameState, Placement, SearchAction};
use tetrisBot::movegen::find_placements;
use tetrisEngine::{
    piece_id, B2BMode, Board, Piece, PieceKind, PostLockPrediction, SpinMode, TetrisEngine,
    BOARD_WIDTH, SPAWN_X, SPAWN_Y,
};

fn engine_from_state(state: &GameState) -> TetrisEngine {
    let mut engine = TetrisEngine::with_seed_and_modes(0, SpinMode::AllSpin, state.b2b_mode);
    engine.board = state.board;
    engine.hold = state.reserve.map(piece_id);
    engine.combo = state.combo;
    engine.combo_active = state.combo_active;
    engine.b2b_chain = state.b2b_chain;
    engine.surge_charge = state.surge_charge;
    engine
}

fn assert_prediction_matches(
    state: GameState,
    action: SearchAction,
    prediction: PostLockPrediction,
) -> GameState {
    let mut advanced = state;
    let info = advanced.advance_action(action);

    assert_eq!(advanced.board, prediction.board);
    assert_eq!(info.lines_cleared as i32, prediction.stats.lines_cleared);
    assert_eq!(
        info.attack, prediction.stats.attack,
        "attack mismatch for action {:?}",
        action
    );
    assert_eq!(info.b2b_chain, prediction.stats.b2b_chain);
    assert_eq!(info.b2b_bonus, prediction.stats.b2b_bonus);
    assert_eq!(info.surge_charge, prediction.stats.surge_charge);
    assert_eq!(info.surge_send, prediction.stats.surge_send);
    assert_eq!(info.combo, prediction.stats.combo);
    assert_eq!(info.combo_active, prediction.stats.combo_active);
    assert_eq!(info.perfect_clear, prediction.stats.perfect_clear);
    assert_eq!(
        info.is_spin, prediction.stats.is_spin,
        "spin mismatch for action {:?}",
        action
    );
    assert_eq!(info.is_mini, prediction.stats.is_mini);
    assert_eq!(info.is_difficult, prediction.stats.is_difficult);
    assert_eq!(info.base_attack, prediction.stats.base_attack);
    assert_eq!(info.combo_attack, prediction.stats.combo_attack);
    assert_eq!(info.placed_kind, Some(action.played_kind()));
    assert_eq!(info.used_hold, action.used_hold());

    advanced
}

fn assert_prediction_parity(state: GameState, action: SearchAction) -> GameState {
    let engine = engine_from_state(&state);
    let prediction = engine.predict_post_lock_stats(&action.placement.to_piece(), None);
    assert_prediction_matches(state, action, prediction)
}

fn assert_empty_hold_parity(
    state: GameState,
    incoming_kind: PieceKind,
    placement: Placement,
) -> GameState {
    let mut engine = engine_from_state(&state);
    engine.current_piece = Some(Piece::new(incoming_kind, 0, (SPAWN_X, SPAWN_Y)));
    engine.bag = vec![piece_id(placement.kind)];
    assert!(engine.hold_current());

    let prediction = engine.predict_post_lock_stats(&placement.to_piece(), None);
    assert_prediction_matches(
        state,
        SearchAction::hold_empty(incoming_kind, placement),
        prediction,
    )
}

fn board_from_heights(heights: [usize; BOARD_WIDTH]) -> Board {
    let mut board = [0i8; 400];
    for (x, &height) in heights.iter().enumerate() {
        for depth in 0..height {
            let row = 39 - depth;
            board[row * BOARD_WIDTH + x] = 8;
        }
    }
    board
}

#[test]
fn parity_matches_engine_for_all_empty_board_movegen_placements() {
    let board = [0i8; 400];

    for kind in [
        PieceKind::I,
        PieceKind::O,
        PieceKind::T,
        PieceKind::S,
        PieceKind::Z,
        PieceKind::J,
        PieceKind::L,
    ] {
        for (placement, _) in find_placements(&board, kind) {
            let mut state = GameState::empty();
            state.board = board;
            state.bag = BagSet::full();
            let _ = assert_prediction_parity(state, SearchAction::play_current(kind, placement));
        }
    }
}

#[test]
fn parity_matches_engine_for_generated_stack_placements() {
    let board = board_from_heights([0, 2, 4, 1, 3, 0, 5, 2, 1, 0]);

    for kind in [
        PieceKind::I,
        PieceKind::O,
        PieceKind::T,
        PieceKind::S,
        PieceKind::Z,
        PieceKind::J,
        PieceKind::L,
    ] {
        for (placement, _) in find_placements(&board, kind) {
            let mut state = GameState::empty();
            state.board = board;
            state.b2b_chain = 4;
            state.surge_charge = 3;
            state.combo = 2;
            state.combo_active = true;
            let _ = assert_prediction_parity(state, SearchAction::play_current(kind, placement));
        }
    }
}

#[test]
fn parity_matches_targeted_t_spin_and_all_spin_cases() {
    let t_spin = Placement {
        x: 4,
        y: 4,
        rotation: 0,
        kind: PieceKind::T,
        last_was_rot: true,
        last_rot_dir: Some(1),
        last_kick_idx: Some(0),
        is_spin: true,
        is_mini: false,
    };
    let mut t_board = [0i8; 400];
    t_board[4 * BOARD_WIDTH + 4] = 9;
    t_board[4 * BOARD_WIDTH + 6] = 9;
    t_board[6 * BOARD_WIDTH + 4] = 9;

    let mut t_state = GameState::empty();
    t_state.board = t_board;
    let _ = assert_prediction_parity(t_state, SearchAction::play_current(PieceKind::T, t_spin));

    let j_spin = Placement {
        x: 4,
        y: 4,
        rotation: 0,
        kind: PieceKind::J,
        last_was_rot: true,
        last_rot_dir: Some(1),
        last_kick_idx: Some(0),
        is_spin: true,
        is_mini: true,
    };
    let mut j_board = [0i8; 400];
    j_board[4 * BOARD_WIDTH + 3] = 9;
    j_board[4 * BOARD_WIDTH + 5] = 9;

    let mut j_state = GameState::empty();
    j_state.board = j_board;
    let _ = assert_prediction_parity(j_state, SearchAction::play_current(PieceKind::J, j_spin));
}

#[test]
fn hold_swap_actions_match_engine_prediction_and_update_reserve() {
    let placement = Placement {
        x: 4,
        y: 37,
        rotation: 0,
        kind: PieceKind::O,
        last_was_rot: false,
        last_rot_dir: None,
        last_kick_idx: None,
        is_spin: false,
        is_mini: false,
    };

    let mut state = GameState::empty();
    state.reserve = Some(PieceKind::O);
    state.bag = BagSet::full();
    let advanced = assert_prediction_parity(state, SearchAction::use_hold(PieceKind::T, placement));

    assert_eq!(advanced.reserve, Some(PieceKind::T));
    assert!(!advanced.bag.contains(PieceKind::T));
    assert!(advanced.bag.contains(PieceKind::O));
}

#[test]
fn empty_hold_actions_match_engine_prediction_and_update_reserve() {
    let placement = Placement {
        x: 4,
        y: 37,
        rotation: 0,
        kind: PieceKind::O,
        last_was_rot: false,
        last_rot_dir: None,
        last_kick_idx: None,
        is_spin: false,
        is_mini: false,
    };

    let mut state = GameState::empty();
    state.bag = BagSet::full();
    let advanced = assert_empty_hold_parity(state, PieceKind::T, placement);

    assert_eq!(advanced.reserve, Some(PieceKind::T));
    assert!(!advanced.bag.contains(PieceKind::T));
    assert!(!advanced.bag.contains(PieceKind::O));
}

#[test]
fn empty_hold_actions_refill_when_current_piece_ends_the_bag() {
    let placement = Placement {
        x: 4,
        y: 37,
        rotation: 0,
        kind: PieceKind::O,
        last_was_rot: false,
        last_rot_dir: None,
        last_kick_idx: None,
        is_spin: false,
        is_mini: false,
    };

    let mut state = GameState::empty();
    state.bag = BagSet::single(PieceKind::T);
    let advanced = assert_empty_hold_parity(state, PieceKind::T, placement);

    assert_eq!(advanced.reserve, Some(PieceKind::T));
    assert!(advanced.bag.contains(PieceKind::T));
    assert!(!advanced.bag.contains(PieceKind::O));
    assert_eq!(advanced.bag.iter().count(), 6);
}

#[test]
fn b2b_mode_variants_match_engine_prediction() {
    let board = board_from_heights([0, 0, 0, 4, 4, 4, 0, 0, 0, 0]);

    for b2b_mode in [B2BMode::Surge, B2BMode::Chaining] {
        for (placement, _) in find_placements(&board, PieceKind::I) {
            let mut state = GameState::empty();
            state.board = board;
            state.b2b_mode = b2b_mode;
            state.b2b_chain = 5;
            state.surge_charge = 4;
            state.combo = 1;
            state.combo_active = true;
            let _ = assert_prediction_parity(
                state,
                SearchAction::play_current(PieceKind::I, placement),
            );
        }
    }
}
