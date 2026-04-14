use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::time::Instant;

use crate::bot::{Bot, BotConfig, Statistics};
use crate::data::Placement;
use tetrisEngine::PieceKind;

pub struct BotSynchronizer {
    state: parking_lot::Mutex<SyncState>,
    wakeup: parking_lot::Condvar,
    bot: parking_lot::RwLock<Option<Bot>>,
    config: std::sync::Arc<BotConfig>,
}

struct SyncState {
    running: bool,
    shutdown: bool,
    generation: u64,
    session_start: Instant,
    session_stats: Statistics,
}

impl BotSynchronizer {
    pub fn new(config: std::sync::Arc<BotConfig>) -> Self {
        BotSynchronizer {
            state: parking_lot::Mutex::new(SyncState {
                running: false,
                shutdown: false,
                generation: 0,
                session_start: Instant::now(),
                session_stats: Statistics::default(),
            }),
            wakeup: parking_lot::Condvar::new(),
            bot: parking_lot::RwLock::new(None),
            config,
        }
    }

    fn bump_generation(state: &mut SyncState) -> u64 {
        state.generation = state.generation.wrapping_add(1);
        state.session_start = Instant::now();
        state.session_stats = Statistics::default();
        state.generation
    }

    pub fn start(&self, bot: Bot) {
        let generation = {
            let mut state = self.state.lock();
            state.running = true;
            state.shutdown = false;
            Self::bump_generation(&mut state)
        };
        {
            let mut guard = self.bot.write();
            *guard = Some(bot);
        }
        self.prime_generation(generation);
        self.wakeup.notify_all();
    }

    pub fn stop(&self) {
        {
            let mut state = self.state.lock();
            state.running = false;
        }
        {
            let mut guard = self.bot.write();
            *guard = None;
        }
        self.wakeup.notify_all();
    }

    pub fn shutdown(&self) {
        {
            let mut state = self.state.lock();
            state.running = false;
            state.shutdown = true;
        }
        {
            let mut guard = self.bot.write();
            *guard = None;
        }
        self.wakeup.notify_all();
    }

    pub fn suggest(&self) -> Option<(Vec<Placement>, MoveInfo)> {
        self.suggest_impl(self.config.suggest_budget_ms)
    }

    pub fn suggest_with_budget(&self, budget_ms: u64) -> Option<(Vec<Placement>, MoveInfo)> {
        self.suggest_impl(budget_ms)
    }

    fn suggest_impl(&self, budget_ms: u64) -> Option<(Vec<Placement>, MoveInfo)> {
        let guard = self.bot.read();
        let bot = guard.as_ref()?;

        let (generation, min_nodes) = {
            let state = self.state.lock();
            (state.generation, self.config.suggest_min_nodes)
        };

        let deadline = Instant::now() + std::time::Duration::from_millis(budget_ms);

        let mut moves = bot.suggest();
        let mut did_inline_work = false;

        loop {
            let (generation_changed, node_count) = {
                let state = self.state.lock();
                (state.generation != generation, state.session_stats.nodes)
            };

            if generation_changed {
                break;
            }

            let reached_min_nodes = node_count >= min_nodes;
            let timed_out = Instant::now() >= deadline;

            if !moves.is_empty() && (reached_min_nodes || timed_out) {
                break;
            }

            if moves.is_empty() && did_inline_work && timed_out {
                break;
            }

            let stats = bot.do_work();
            did_inline_work = true;
            {
                let mut state = self.state.lock();
                if state.generation == generation {
                    state.session_stats.accumulate(stats);
                }
            }
            moves = bot.suggest();
        }

        drop(guard);

        let state = self.state.lock();
        if state.generation != generation {
            return Some((
                moves,
                MoveInfo {
                    nodes: 0,
                    nps: 0.0,
                    extra: String::new(),
                },
            ));
        }

        Some((moves, Self::build_move_info(&state)))
    }

    pub fn peek(&self) -> Option<(Vec<Placement>, MoveInfo)> {
        let guard = self.bot.read();
        let bot = guard.as_ref()?;
        let moves = bot.suggest();
        drop(guard);

        let state = self.state.lock();
        Some((moves, Self::build_move_info(&state)))
    }

    pub fn advance(&self, mv: Placement) {
        {
            let mut guard = self.bot.write();
            if let Some(bot) = guard.as_mut() {
                bot.advance(mv);
            }
        }
        let generation = {
            let mut state = self.state.lock();
            Self::bump_generation(&mut state)
        };
        self.prime_generation(generation);
    }

    pub fn new_piece(&self, piece: PieceKind) {
        {
            let mut guard = self.bot.write();
            if let Some(bot) = guard.as_mut() {
                bot.new_piece(piece);
            }
        }
        let generation = {
            let mut state = self.state.lock();
            Self::bump_generation(&mut state)
        };
        self.prime_generation(generation);
    }

    pub fn advance_with_pieces(&self, mv: Placement, pieces: Vec<PieceKind>) {
        {
            let mut guard = self.bot.write();
            if let Some(bot) = guard.as_mut() {
                bot.advance(mv);
                for piece in &pieces {
                    bot.new_piece(*piece);
                }
            }
        }
        let generation = {
            let mut state = self.state.lock();
            Self::bump_generation(&mut state)
        };
        self.prime_generation(generation);
    }

    pub fn work_loop(&self, gen_cookie: &AtomicU64) {
        loop {
            {
                let mut state = self.state.lock();
                while !state.running && !state.shutdown {
                    self.wakeup.wait(&mut state);
                }
                if state.shutdown {
                    return;
                }
                gen_cookie.store(state.generation, AtomicOrdering::Relaxed);
            }

            let stats = {
                let guard = self.bot.read();
                match guard.as_ref() {
                    Some(bot) => bot.do_work(),
                    None => continue,
                }
            };

            let my_gen = gen_cookie.load(AtomicOrdering::Relaxed);
            let mut state = self.state.lock();
            if state.shutdown {
                return;
            }
            if !state.running {
                continue;
            }
            if state.generation == my_gen {
                state.session_stats.accumulate(stats);
            }
        }
    }

    fn build_move_info(state: &SyncState) -> MoveInfo {
        let elapsed = state.session_start.elapsed().as_secs_f64();
        let nps = if elapsed > 0.0 {
            state.session_stats.nodes as f64 / elapsed
        } else {
            0.0
        };

        MoveInfo {
            nodes: state.session_stats.nodes,
            nps,
            extra: format!(
                "gen={} sel={} exp={} mg={}c/{}us slot={}c/{}us",
                state.generation,
                state.session_stats.selections,
                state.session_stats.expansions,
                state.session_stats.movegen_calls,
                state.session_stats.movegen_nanos / 1_000,
                state.session_stats.slot_calls,
                state.session_stats.slot_nanos / 1_000,
            ),
        }
    }

    fn prime_generation(&self, generation: u64) {
        let stats = {
            let guard = self.bot.read();
            match guard.as_ref() {
                Some(bot) => bot.do_work(),
                None => return,
            }
        };

        let mut state = self.state.lock();
        if state.generation == generation {
            state.session_stats.accumulate(stats);
        }
    }
}

pub struct MoveInfo {
    pub nodes: u64,
    pub nps: f64,
    pub extra: String,
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::bot::{BotConfig, BotOptions};
    use crate::data::GameState;

    use super::*;

    fn test_config() -> Arc<BotConfig> {
        let mut config = BotConfig::default();
        config.worker_count = 2;
        config.suggest_budget_ms = 200;
        config.suggest_min_nodes = 10;
        Arc::new(config)
    }

    fn test_options(config: &Arc<BotConfig>) -> BotOptions {
        BotOptions {
            speculate: true,
            config: config.clone(),
        }
    }

    #[test]
    fn suggest_warms_up_new_bot_until_moves_exist() {
        let config = test_config();
        let sync = BotSynchronizer::new(config.clone());
        let bot = Bot::new(
            test_options(&config),
            GameState::empty(),
            &[
                PieceKind::S,
                PieceKind::O,
                PieceKind::T,
                PieceKind::I,
                PieceKind::L,
                PieceKind::Z,
            ],
        );

        sync.start(bot);
        let (moves, info) = sync.suggest().expect("started bot should answer suggest");

        assert!(
            !moves.is_empty(),
            "suggest should produce at least one move"
        );
        assert!(info.nodes > 0, "should have expanded at least one node");
    }

    #[test]
    fn suggest_does_inline_work_when_moves_already_exist_and_min_nodes_requested() {
        let config = {
            let mut config = BotConfig::default();
            config.worker_count = 1;
            config.suggest_budget_ms = 5_000;
            config.suggest_min_nodes = 1;
            Arc::new(config)
        };
        let sync = BotSynchronizer::new(config.clone());
        let bot = Bot::new(
            test_options(&config),
            GameState::empty(),
            &[
                PieceKind::S,
                PieceKind::O,
                PieceKind::T,
                PieceKind::I,
                PieceKind::L,
                PieceKind::Z,
            ],
        );

        let pre_stats = bot.do_work();
        assert!(
            pre_stats.nodes > 0,
            "precondition: bot should expand before start"
        );
        assert!(
            !bot.suggest().is_empty(),
            "precondition: bot should already have moves before sync.start"
        );

        sync.start(bot);
        let (_, info) = sync.suggest().expect("started bot should answer suggest");

        assert!(
            info.nodes > 0,
            "suggest should do inline search work when moves already exist and min_nodes is requested: got {}",
            info.nodes,
        );
    }

    #[test]
    fn suggest_reports_positive_nps_after_work() {
        let config = test_config();
        let sync = BotSynchronizer::new(config.clone());
        let bot = Bot::new(
            test_options(&config),
            GameState::empty(),
            &[
                PieceKind::S,
                PieceKind::O,
                PieceKind::T,
                PieceKind::I,
                PieceKind::L,
                PieceKind::Z,
            ],
        );

        sync.start(bot);
        let (_, info) = sync.suggest().expect("started bot should answer suggest");

        assert!(
            info.nps > 0.0,
            "nps should be positive after work: got {}",
            info.nps
        );
        assert!(
            info.extra.contains("mg=") && info.extra.contains("slot="),
            "extra info should expose profiling counters: {}",
            info.extra,
        );
    }

    #[test]
    fn peek_returns_immediate_snapshot() {
        let config = test_config();
        let sync = BotSynchronizer::new(config.clone());
        let bot = Bot::new(
            test_options(&config),
            GameState::empty(),
            &[
                PieceKind::S,
                PieceKind::O,
                PieceKind::T,
                PieceKind::I,
                PieceKind::L,
                PieceKind::Z,
            ],
        );

        sync.start(bot);
        let (_moves, info) = sync.peek().expect("started bot should answer peek");

        assert!(
            info.extra.contains("gen="),
            "peek should return move info metadata: {}",
            info.extra,
        );
    }

    #[test]
    fn stats_reset_on_advance() {
        let config = test_config();
        let sync = BotSynchronizer::new(config.clone());
        let bot = Bot::new(
            test_options(&config),
            GameState::empty(),
            &[
                PieceKind::S,
                PieceKind::O,
                PieceKind::T,
                PieceKind::I,
                PieceKind::L,
                PieceKind::Z,
            ],
        );

        sync.start(bot);
        let (_, info1) = sync.suggest().expect("should answer first suggest");
        assert!(info1.nodes > 0);

        sync.advance(Placement {
            x: 3,
            y: 18,
            rotation: 0,
            kind: PieceKind::S,
            last_was_rot: false,
            last_rot_dir: None,
            last_kick_idx: None,
            is_spin: false,
            is_mini: false,
        });

        let (_, info2) = sync.suggest().expect("should answer suggest after advance");
        assert!(
            info2.nodes < info1.nodes,
            "stats should reset on advance: before={} after={}",
            info1.nodes,
            info2.nodes,
        );
    }

    #[test]
    fn stats_reset_on_new_piece() {
        let config = test_config();
        let sync = BotSynchronizer::new(config.clone());
        let bot = Bot::new(
            test_options(&config),
            GameState::empty(),
            &[
                PieceKind::S,
                PieceKind::O,
                PieceKind::T,
                PieceKind::I,
                PieceKind::L,
                PieceKind::Z,
            ],
        );

        sync.start(bot);
        let (_, info1) = sync.suggest().expect("should answer first suggest");
        assert!(info1.nodes > 0);

        sync.new_piece(PieceKind::I);

        let (_, info2) = sync
            .suggest()
            .expect("should answer suggest after new_piece");
        assert!(
            info2.nodes < info1.nodes,
            "stats should reset on new_piece: before={} after={}",
            info1.nodes,
            info2.nodes,
        );
    }

    #[test]
    fn multi_worker_no_panic() {
        let config = {
            let mut c = BotConfig::default();
            c.worker_count = 4;
            c.suggest_budget_ms = 15000;
            c.suggest_min_nodes = 1;
            Arc::new(c)
        };
        let sync = Arc::new(BotSynchronizer::new(config.clone()));
        let bot = Bot::new(
            BotOptions {
                speculate: true,
                config: config.clone(),
            },
            GameState::empty(),
            &[
                PieceKind::S,
                PieceKind::O,
                PieceKind::T,
                PieceKind::I,
                PieceKind::L,
                PieceKind::Z,
            ],
        );

        sync.start(bot);

        let handles: Vec<_> = (0..4)
            .map(|_| {
                let sync = sync.clone();
                std::thread::spawn(move || {
                    let gen_cookie = AtomicU64::new(0);
                    sync.work_loop(&gen_cookie);
                })
            })
            .collect();

        let (moves, info) = sync.suggest().expect("should answer suggest");
        assert!(
            !moves.is_empty(),
            "suggest should produce moves with worker pool"
        );
        assert!(
            info.nodes > 0,
            "should have expanded nodes: got {}",
            info.nodes
        );

        sync.shutdown();
        for handle in handles {
            let _ = handle.join();
        }
    }

    #[test]
    fn workers_survive_stop_and_restart() {
        let config = {
            let mut c = BotConfig::default();
            c.worker_count = 4;
            c.suggest_budget_ms = 15_000;
            c.suggest_min_nodes = 1;
            Arc::new(c)
        };
        let sync = Arc::new(BotSynchronizer::new(config.clone()));

        let handles: Vec<_> = (0..4)
            .map(|_| {
                let sync = sync.clone();
                std::thread::spawn(move || {
                    let gen_cookie = AtomicU64::new(0);
                    sync.work_loop(&gen_cookie);
                })
            })
            .collect();

        let make_bot = || {
            Bot::new(
                BotOptions {
                    speculate: true,
                    config: config.clone(),
                },
                GameState::empty(),
                &[
                    PieceKind::S,
                    PieceKind::O,
                    PieceKind::T,
                    PieceKind::I,
                    PieceKind::L,
                    PieceKind::Z,
                ],
            )
        };

        sync.start(make_bot());
        let (moves1, info1) = sync.suggest().expect("should answer first suggest");
        assert!(!moves1.is_empty(), "first suggest should produce moves");
        assert!(info1.nodes > 0, "first suggest should expand nodes");

        sync.stop();
        sync.start(make_bot());
        let (moves2, info2) = sync.suggest().expect("should answer second suggest");
        assert!(!moves2.is_empty(), "second suggest should produce moves");
        assert!(info2.nodes > 0, "second suggest should expand nodes");

        sync.shutdown();
        for handle in handles {
            let _ = handle.join();
        }
    }
}
