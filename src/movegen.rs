use std::collections::{HashMap, VecDeque};

use tetrisEngine::{
    board_index, cell_blocked, compute_blocks, is_position_valid, rotation_candidates, Board,
    Piece, PieceKind, BOARD_HEIGHT, BOARD_WIDTH, SPAWN_X, SPAWN_Y,
};

use crate::data::Placement;

pub fn find_placements(board: &Board, kind: PieceKind) -> Vec<(Placement, u32)> {
    let spawn_x = SPAWN_X;
    let spawn_y = SPAWN_Y;

    if !is_position_valid(board, &Piece::new(kind, 0, (spawn_x, spawn_y)), None, None) {
        return Vec::new();
    }

    let mut queue = VecDeque::new();
    let start_state = SearchState {
        x: spawn_x,
        y: spawn_y,
        rotation: 0,
        last_was_rot: false,
        last_rot_dir: None,
        last_kick_idx: None,
        soft_drop: 0,
    };
    queue.push_back(start_state);

    let mut best_soft_drop: HashMap<SearchStateKey, u32> = HashMap::new();
    best_soft_drop.insert(start_state.key(), 0);

    let mut terminals: std::collections::HashMap<(i16, i16, u8, bool, bool), SearchState> =
        std::collections::HashMap::new();
    let rotation_actions: &[i8] = &[1, -1, 2];
    let probe_piece = Piece::new(kind, 0, (spawn_x, spawn_y));

    while let Some(state) = queue.pop_front() {
        let dropped = hard_drop_state(board, &probe_piece, state);
        let (is_spin, is_mini) = detect_spin_at_terminal(board, kind, &dropped);
        let key = (dropped.x, dropped.y, dropped.rotation, is_spin, is_mini);
        match terminals.get_mut(&key) {
            Some(existing) if dropped.soft_drop < existing.soft_drop => *existing = dropped,
            None => {
                terminals.insert(key, dropped);
            }
            _ => {}
        }

        let can_move_down = is_position_valid(
            board,
            &probe_piece,
            Some((state.x, state.y + 1)),
            Some(state.rotation),
        );

        for (dx, dy) in [(-1_i16, 0_i16), (1, 0)] {
            let nx = state.x + dx;
            let ny = state.y + dy;
            if !is_position_valid(board, &probe_piece, Some((nx, ny)), Some(state.rotation)) {
                continue;
            }
            let next_state = SearchState {
                x: nx,
                y: ny,
                rotation: state.rotation,
                last_was_rot: false,
                last_rot_dir: None,
                last_kick_idx: None,
                soft_drop: state.soft_drop,
            };
            if !should_visit(&mut best_soft_drop, next_state) {
                continue;
            }
            queue.push_front(next_state);
        }

        if can_move_down {
            let next_state = SearchState {
                x: state.x,
                y: state.y + 1,
                rotation: state.rotation,
                last_was_rot: false,
                last_rot_dir: None,
                last_kick_idx: None,
                soft_drop: state.soft_drop + 1,
            };
            if should_visit(&mut best_soft_drop, next_state) {
                queue.push_back(next_state);
            }
        }

        for &rot_dir in rotation_actions {
            let new_rotation =
                ((i16::from(state.rotation) + i16::from(rot_dir)).rem_euclid(4)) as u8;

            let mut success = false;
            let mut final_x = state.x;
            let mut final_y = state.y;
            let mut final_kick_idx = 0_u8;

            let is_o = kind == PieceKind::O;

            if rot_dir.abs() != 2 && is_o {
                if is_position_valid(
                    board,
                    &probe_piece,
                    Some((state.x, state.y)),
                    Some(new_rotation),
                ) {
                    success = true;
                }
            } else {
                for (kick_idx, kick_x, kick_y) in
                    rotation_candidates(kind, state.rotation, new_rotation, rot_dir)
                {
                    let tx = state.x + i16::from(kick_x);
                    let ty = state.y - i16::from(kick_y);
                    if is_position_valid(board, &probe_piece, Some((tx, ty)), Some(new_rotation)) {
                        success = true;
                        final_x = tx;
                        final_y = ty;
                        final_kick_idx = kick_idx;
                        break;
                    }
                }
            }

            if !success {
                continue;
            }

            let next_state = SearchState {
                x: final_x,
                y: final_y,
                rotation: new_rotation,
                last_was_rot: true,
                last_rot_dir: Some(rot_dir),
                last_kick_idx: Some(final_kick_idx),
                soft_drop: state.soft_drop,
            };
            if !should_visit(&mut best_soft_drop, next_state) {
                continue;
            }
            queue.push_front(next_state);
        }
    }

    let mut results: Vec<(Placement, u32)> = Vec::with_capacity(terminals.len());

    for terminal in terminals.into_values() {
        let (is_spin, is_mini) = detect_spin_at_terminal(board, kind, &terminal);

        results.push((
            Placement {
                x: terminal.x,
                y: terminal.y,
                rotation: terminal.rotation,
                kind,
                last_was_rot: terminal.last_was_rot,
                last_rot_dir: terminal.last_rot_dir,
                last_kick_idx: terminal.last_kick_idx,
                is_spin,
                is_mini,
            },
            terminal.soft_drop,
        ));
    }

    results
}

fn hard_drop_state(board: &Board, probe_piece: &Piece, mut state: SearchState) -> SearchState {
    while is_position_valid(
        board,
        probe_piece,
        Some((state.x, state.y + 1)),
        Some(state.rotation),
    ) {
        state.y += 1;
    }
    state
}

fn detect_spin_at_terminal(board: &Board, kind: PieceKind, state: &SearchState) -> (bool, bool) {
    if !state.last_was_rot {
        return (false, false);
    }

    if kind == PieceKind::O {
        return (false, false);
    }

    let piece = Piece::new(kind, state.rotation, (state.x, state.y));
    let immobile = is_piece_immobile(board, &piece);

    if !immobile {
        return (false, false);
    }

    if kind == PieceKind::T {
        let corners = occupied_3x3_corners(board, &piece);
        if corners >= 3 {
            let is_180 = state.last_rot_dir.unwrap_or(0).abs() == 2;
            let front_corners = count_t_front_corners(board, &piece);
            let is_full = is_180 || state.last_kick_idx == Some(4) || front_corners == 2;
            return (true, !is_full);
        }
        return (true, true);
    }

    (true, true)
}

pub fn is_piece_immobile(board: &Board, piece: &Piece) -> bool {
    let (px, py) = piece.position;
    !is_position_valid(board, piece, Some((px - 1, py)), None)
        && !is_position_valid(board, piece, Some((px + 1, py)), None)
        && !is_position_valid(board, piece, Some((px, py - 1)), None)
}

fn occupied_3x3_corners(board: &Board, piece: &Piece) -> u8 {
    let (px, py) = piece.position;
    [(0_i16, 0_i16), (2, 0), (0, 2), (2, 2)]
        .into_iter()
        .filter(|(cx, cy)| cell_blocked(board, px + cx, py + cy))
        .count() as u8
}

fn count_t_front_corners(board: &Board, piece: &Piece) -> u8 {
    let (px, py) = piece.position;
    let corners = match piece.rotation % 4 {
        0 => [(0_i16, 0_i16), (2, 0)],
        1 => [(2_i16, 0_i16), (2, 2)],
        2 => [(0_i16, 2_i16), (2, 2)],
        _ => [(0_i16, 0_i16), (0, 2)],
    };
    corners
        .into_iter()
        .filter(|(cx, cy)| cell_blocked(board, px + cx, py + cy))
        .count() as u8
}

fn should_visit(best_soft_drop: &mut HashMap<SearchStateKey, u32>, state: SearchState) -> bool {
    let key = state.key();
    match best_soft_drop.get_mut(&key) {
        Some(best) if state.soft_drop < *best => {
            *best = state.soft_drop;
            true
        }
        None => {
            best_soft_drop.insert(key, state.soft_drop);
            true
        }
        _ => false,
    }
}

#[derive(Clone, Copy, Debug)]
struct SearchState {
    x: i16,
    y: i16,
    rotation: u8,
    last_was_rot: bool,
    last_rot_dir: Option<i8>,
    last_kick_idx: Option<u8>,
    soft_drop: u32,
}

impl SearchState {
    fn key(self) -> SearchStateKey {
        SearchStateKey {
            x: self.x,
            y: self.y,
            rotation: self.rotation,
            last_was_rot: self.last_was_rot,
            last_rot_dir: self.last_rot_dir,
            last_kick_idx: self.last_kick_idx,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
struct SearchStateKey {
    x: i16,
    y: i16,
    rotation: u8,
    last_was_rot: bool,
    last_rot_dir: Option<i8>,
    last_kick_idx: Option<u8>,
}

pub fn simulate_lock_line_count(board: &Board, kind: PieceKind, placement: &Placement) -> u32 {
    let mut board = *board;
    let id = tetrisEngine::piece_id(kind);
    let piece = Piece::new(kind, placement.rotation, (placement.x, placement.y));
    for (x, y) in compute_blocks(&piece, None, None) {
        if let Some(idx) = board_index(x, y) {
            board[idx] = id;
        }
    }

    let mut count = 0u32;
    for row in 0..BOARD_HEIGHT {
        let start = row * BOARD_WIDTH;
        let end = start + BOARD_WIDTH;
        if board[start..end].iter().all(|&c| c != 0) {
            count += 1;
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_placements_empty_board_returns_non_empty() {
        let board = [0i8; BOARD_WIDTH * BOARD_HEIGHT];
        for kind in [
            PieceKind::I,
            PieceKind::O,
            PieceKind::T,
            PieceKind::S,
            PieceKind::Z,
            PieceKind::J,
            PieceKind::L,
        ] {
            let placements = find_placements(&board, kind);
            assert!(!placements.is_empty(), "No placements for {:?}", kind);
        }
    }

    #[test]
    fn soft_drop_distances_non_negative() {
        let board = [0i8; BOARD_WIDTH * BOARD_HEIGHT];
        for kind in [
            PieceKind::I,
            PieceKind::O,
            PieceKind::T,
            PieceKind::S,
            PieceKind::Z,
            PieceKind::J,
            PieceKind::L,
        ] {
            for (placement, sd) in find_placements(&board, kind) {
                assert!(
                    sd == 0,
                    "Flat board placement should have sd=0, got {} for {:?} at ({},{})",
                    sd,
                    placement.kind,
                    placement.x,
                    placement.y
                );
            }
        }
    }

    #[test]
    fn all_placements_valid_on_empty_board() {
        let board = [0i8; BOARD_WIDTH * BOARD_HEIGHT];
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
                let piece = Piece::new(kind, placement.rotation, (placement.x, placement.y));
                assert!(
                    is_position_valid(&board, &piece, None, None),
                    "Invalid placement: {:?} at ({},{}) rot {}",
                    kind,
                    placement.x,
                    placement.y,
                    placement.rotation
                );
            }
        }
    }

    #[test]
    fn no_spin_on_flat_board() {
        let board = [0i8; BOARD_WIDTH * BOARD_HEIGHT];
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
                assert!(
                    !placement.is_spin,
                    "Spin on flat board for {:?} at ({},{}) rot {}",
                    kind, placement.x, placement.y, placement.rotation
                );
            }
        }
    }

    #[test]
    fn placements_are_unique_by_position_rotation_spin() {
        let board = [0i8; BOARD_WIDTH * BOARD_HEIGHT];
        for kind in [
            PieceKind::I,
            PieceKind::O,
            PieceKind::T,
            PieceKind::S,
            PieceKind::Z,
            PieceKind::J,
            PieceKind::L,
        ] {
            let placements = find_placements(&board, kind);
            let mut seen = std::collections::HashSet::new();
            for (p, _) in &placements {
                let key = (p.x, p.y, p.rotation, p.is_spin, p.is_mini);
                assert!(seen.insert(key), "Duplicate placement for {:?}", kind);
            }
        }
    }

    #[test]
    fn immobility_detection_works() {
        let mut board = [0i8; BOARD_WIDTH * BOARD_HEIGHT];
        let piece = Piece::new(PieceKind::T, 2, (5, 38));

        board[39 * BOARD_WIDTH + 4] = 1;
        board[39 * BOARD_WIDTH + 5] = 1;
        board[39 * BOARD_WIDTH + 6] = 1;
        board[38 * BOARD_WIDTH + 4] = 1;
        board[38 * BOARD_WIDTH + 6] = 1;

        assert!(is_piece_immobile(&board, &piece));
    }

    #[test]
    fn simulate_lock_counts_lines() {
        let mut board = [0i8; BOARD_WIDTH * BOARD_HEIGHT];
        for x in 0..BOARD_WIDTH {
            board[39 * BOARD_WIDTH + x] = 1;
            board[38 * BOARD_WIDTH + x] = 1;
            board[37 * BOARD_WIDTH + x] = 1;
        }
        for x in 0..3 {
            board[36 * BOARD_WIDTH + x] = 1;
        }
        for x in 7..BOARD_WIDTH {
            board[36 * BOARD_WIDTH + x] = 1;
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
        let lines = simulate_lock_line_count(&board, PieceKind::I, &placement);
        assert_eq!(lines, 4);
    }
}
