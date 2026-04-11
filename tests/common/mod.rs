#![allow(dead_code)]

use std::sync::Arc;

use tetrisBot::bot::{Bot, BotConfig, BotOptions, Statistics};
use tetrisBot::data::{BagSet, GameState, Placement};
use tetrisBot::piece_map::ALL_KINDS;
use tetrisEngine::{
    piece_id, B2BMode, PieceKind, PlacementPayload, SpinMode, TetrisEngine, BOARD_HEIGHT,
    BOARD_WIDTH,
};

pub const DEFAULT_PREVIEW: usize = 6;

#[derive(Clone, Debug)]
pub struct SnapshotCase {
    pub name: &'static str,
    pub visible_rows_bottom_up: &'static [&'static str],
    pub queue: &'static [PieceKind],
    pub reserve: Option<PieceKind>,
    pub bag: &'static [PieceKind],
    pub b2b_chain: i32,
    pub surge_charge: i32,
    pub combo: i32,
    pub combo_active: bool,
    pub b2b_mode: B2BMode,
    pub work_iterations: usize,
    pub top_n: usize,
}

#[derive(Clone, Debug)]
pub struct SeedRunSummary {
    pub seed: u64,
    pub pieces_placed: usize,
    pub total_attack: i32,
    pub total_nodes: u64,
    pub invalid_move: bool,
    pub game_over: bool,
    pub game_over_reason: Option<String>,
}

#[derive(Clone, Debug)]
pub struct StrengthSummary {
    pub runs: Vec<SeedRunSummary>,
    pub max_pieces: usize,
}

impl StrengthSummary {
    pub fn seeds_run(&self) -> usize {
        self.runs.len()
    }

    pub fn average_pieces_placed(&self) -> f64 {
        average(self.runs.iter().map(|run| run.pieces_placed as f64))
    }

    pub fn average_attack_per_piece(&self) -> f64 {
        let total_attack: i64 = self
            .runs
            .iter()
            .map(|run| i64::from(run.total_attack))
            .sum();
        let total_pieces: usize = self.runs.iter().map(|run| run.pieces_placed).sum();
        if total_pieces == 0 {
            0.0
        } else {
            total_attack as f64 / total_pieces as f64
        }
    }

    pub fn average_nodes_per_piece(&self) -> f64 {
        let total_nodes: u64 = self.runs.iter().map(|run| run.total_nodes).sum();
        let total_pieces: usize = self.runs.iter().map(|run| run.pieces_placed).sum();
        if total_pieces == 0 {
            0.0
        } else {
            total_nodes as f64 / total_pieces as f64
        }
    }

    pub fn failed_seeds(&self) -> Vec<&SeedRunSummary> {
        self.runs
            .iter()
            .filter(|run| {
                run.invalid_move || (run.game_over && run.pieces_placed < self.max_pieces)
            })
            .collect()
    }
}

pub fn default_options() -> BotOptions {
    BotOptions {
        speculate: true,
        config: Arc::new(BotConfig::default()),
    }
}

pub fn board_from_visible_rows_bottom_up(rows: &[&str]) -> [i8; BOARD_WIDTH * BOARD_HEIGHT] {
    let mut board = [0i8; BOARD_WIDTH * BOARD_HEIGHT];

    for (row_offset, row) in rows.iter().enumerate() {
        assert_eq!(row.len(), BOARD_WIDTH, "row must be exactly 10 columns");
        let y = BOARD_HEIGHT - 1 - row_offset;
        for (x, ch) in row.chars().enumerate() {
            if ch != '.' {
                board[y * BOARD_WIDTH + x] = 8;
            }
        }
    }

    board
}

pub fn bag_from_pieces(pieces: &[PieceKind]) -> BagSet {
    let mut bag = BagSet::empty();
    for &piece in pieces {
        bag.insert(piece);
    }
    bag
}

pub fn snapshot_state(case: &SnapshotCase) -> GameState {
    GameState {
        board: board_from_visible_rows_bottom_up(case.visible_rows_bottom_up),
        bag: bag_from_pieces(case.bag),
        reserve: case.reserve,
        b2b_chain: case.b2b_chain,
        surge_charge: case.surge_charge,
        combo: case.combo,
        combo_active: case.combo_active,
        b2b_mode: case.b2b_mode,
    }
}

pub fn suggestions_for_case(case: &SnapshotCase) -> Vec<Placement> {
    let bot = Bot::new(default_options(), snapshot_state(case), case.queue);
    for _ in 0..case.work_iterations {
        let _ = bot.do_work();
    }
    bot.suggest().into_iter().take(case.top_n).collect()
}

pub fn run_strength_harness(
    seeds: impl IntoIterator<Item = u64>,
    max_pieces: usize,
    work_iterations_per_turn: usize,
    preview_len: usize,
) -> StrengthSummary {
    let runs = seeds
        .into_iter()
        .map(|seed| run_seeded_game(seed, max_pieces, work_iterations_per_turn, preview_len))
        .collect();

    StrengthSummary { runs, max_pieces }
}

pub fn run_seeded_game(
    seed: u64,
    max_pieces: usize,
    work_iterations_per_turn: usize,
    preview_len: usize,
) -> SeedRunSummary {
    let mut engine = TetrisEngine::with_seed_and_modes(seed, SpinMode::AllSpin, B2BMode::Surge);
    assert!(engine.spawn_next(false), "initial spawn must succeed");

    let mut visible_queue = queue_from_engine(&engine, preview_len);
    let mut bot = Bot::new(
        default_options(),
        game_state_from_engine(&engine),
        &visible_queue,
    );
    let mut total_stats = Statistics::default();
    let mut invalid_move = false;

    while !engine.game_over && engine.pieces_placed < max_pieces as i32 {
        for _ in 0..work_iterations_per_turn {
            total_stats.accumulate(bot.do_work());
        }

        let Some(mv) = bot.suggest().into_iter().next() else {
            invalid_move = true;
            break;
        };

        let previous_queue = visible_queue.clone();
        bot.advance(mv);

        if !apply_move_to_engine(&mut engine, mv) {
            invalid_move = true;
            break;
        }

        if engine.game_over || engine.pieces_placed >= max_pieces as i32 {
            break;
        }

        if !engine.spawn_next(false) {
            break;
        }

        visible_queue = queue_from_engine(&engine, preview_len);
        let revealed = revealed_tail(&previous_queue, &visible_queue)
            .expect("queue should shift by one or more positions after each move");
        for piece in revealed {
            bot.new_piece(piece);
        }
    }

    SeedRunSummary {
        seed,
        pieces_placed: engine.pieces_placed.max(0) as usize,
        total_attack: engine.total_attack_sent,
        total_nodes: total_stats.nodes,
        invalid_move,
        game_over: engine.game_over,
        game_over_reason: engine.game_over_reason.clone(),
    }
}

pub fn canonical_snapshot_cases() -> Vec<SnapshotCase> {
    vec![
        SnapshotCase {
            name: "empty_t_opening",
            visible_rows_bottom_up: &[],
            queue: &[
                PieceKind::T,
                PieceKind::I,
                PieceKind::O,
                PieceKind::L,
                PieceKind::J,
                PieceKind::S,
                PieceKind::Z,
            ],
            reserve: None,
            bag: &ALL_KINDS,
            b2b_chain: 0,
            surge_charge: 0,
            combo: 0,
            combo_active: false,
            b2b_mode: B2BMode::Surge,
            work_iterations: 1,
            top_n: 5,
        },
        SnapshotCase {
            name: "right_well_hold_i",
            visible_rows_bottom_up: &["#########.", "#########.", "#########.", "#########."],
            queue: &[
                PieceKind::T,
                PieceKind::O,
                PieceKind::L,
                PieceKind::J,
                PieceKind::S,
                PieceKind::Z,
                PieceKind::I,
            ],
            reserve: Some(PieceKind::I),
            bag: &ALL_KINDS,
            b2b_chain: 2,
            surge_charge: 1,
            combo: 0,
            combo_active: false,
            b2b_mode: B2BMode::Surge,
            work_iterations: 1,
            top_n: 5,
        },
        SnapshotCase {
            name: "t_spin_ready_center",
            visible_rows_bottom_up: &["###.######", "##...#####", "###.######", "##########"],
            queue: &[
                PieceKind::T,
                PieceKind::I,
                PieceKind::O,
                PieceKind::L,
                PieceKind::J,
                PieceKind::S,
                PieceKind::Z,
            ],
            reserve: None,
            bag: &ALL_KINDS,
            b2b_chain: 3,
            surge_charge: 2,
            combo: 1,
            combo_active: true,
            b2b_mode: B2BMode::Surge,
            work_iterations: 1,
            top_n: 5,
        },
        SnapshotCase {
            name: "j_spin_glue_left",
            visible_rows_bottom_up: &[".#########", "..########", ".#########", "##########"],
            queue: &[
                PieceKind::J,
                PieceKind::T,
                PieceKind::O,
                PieceKind::L,
                PieceKind::S,
                PieceKind::Z,
                PieceKind::I,
            ],
            reserve: None,
            bag: &ALL_KINDS,
            b2b_chain: 1,
            surge_charge: 0,
            combo: 0,
            combo_active: false,
            b2b_mode: B2BMode::Surge,
            work_iterations: 1,
            top_n: 5,
        },
        SnapshotCase {
            name: "combo_downstack_with_hold",
            visible_rows_bottom_up: &["##.#######", "##..######", "###.######", "##########"],
            queue: &[
                PieceKind::S,
                PieceKind::Z,
                PieceKind::T,
                PieceKind::I,
                PieceKind::O,
                PieceKind::L,
                PieceKind::J,
            ],
            reserve: Some(PieceKind::T),
            bag: &ALL_KINDS,
            b2b_chain: 0,
            surge_charge: 0,
            combo: 2,
            combo_active: true,
            b2b_mode: B2BMode::Surge,
            work_iterations: 1,
            top_n: 5,
        },
    ]
}

fn apply_move_to_engine(engine: &mut TetrisEngine, placement: Placement) -> bool {
    let current_kind = match engine.current_piece {
        Some(piece) => piece.kind,
        None => return false,
    };

    if current_kind != placement.kind && !engine.hold_current() {
        return false;
    }

    if engine.current_piece.map(|piece| piece.kind) != Some(placement.kind) {
        return false;
    }

    engine
        .execute_placement(placement_payload(placement), false)
        .ok
}

fn placement_payload(placement: Placement) -> PlacementPayload {
    PlacementPayload {
        x: Some(placement.x),
        y: Some(placement.y),
        rotation: Some(placement.rotation),
        last_was_rot: Some(placement.last_was_rot),
        last_rot_dir: placement.last_rot_dir,
        last_kick_idx: placement.last_kick_idx.map(|idx| idx as i8),
    }
}

fn queue_from_engine(engine: &TetrisEngine, preview_len: usize) -> Vec<PieceKind> {
    let snapshot = engine.get_queue_snapshot(preview_len);
    let mut queue = Vec::with_capacity(1 + snapshot.next_kinds.len());
    queue.push(
        snapshot
            .current
            .expect("spawned engine must have a current piece"),
    );
    queue.extend(snapshot.next_kinds);
    queue
}

fn game_state_from_engine(engine: &TetrisEngine) -> GameState {
    let current = engine
        .current_piece
        .expect("spawned engine must have a current piece")
        .kind;
    let counts = engine.get_bag_remainder_counts();
    let mut bag = BagSet::single(current);
    for (idx, &count) in counts.counts.iter().enumerate() {
        if count > 0 {
            bag.insert(ALL_KINDS[idx]);
        }
    }

    GameState {
        board: engine.board,
        bag,
        reserve: engine.hold.map(|id| {
            ALL_KINDS
                .into_iter()
                .find(|&kind| piece_id(kind) == id)
                .expect("hold must contain a valid piece id")
        }),
        b2b_chain: engine.b2b_chain,
        surge_charge: engine.surge_charge,
        combo: engine.combo,
        combo_active: engine.combo_active,
        b2b_mode: engine.b2b_mode,
    }
}

fn revealed_tail(old_queue: &[PieceKind], new_queue: &[PieceKind]) -> Option<Vec<PieceKind>> {
    if old_queue.len() != new_queue.len() {
        return None;
    }

    for shift in 1..=old_queue.len() {
        if old_queue[shift..] == new_queue[..old_queue.len() - shift] {
            return Some(new_queue[old_queue.len() - shift..].to_vec());
        }
    }

    None
}

fn average(values: impl Iterator<Item = f64>) -> f64 {
    let mut sum = 0.0;
    let mut count = 0usize;
    for value in values {
        sum += value;
        count += 1;
    }
    if count == 0 {
        0.0
    } else {
        sum / count as f64
    }
}
