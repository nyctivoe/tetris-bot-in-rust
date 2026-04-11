use crate::bot::{Bot, Statistics};
use crate::data::Placement;
use tetrisEngine::PieceKind;

const SUGGEST_WARMUP_ITERS: usize = 64;

pub struct BotSynchronizer {
    state: parking_lot::Mutex<SyncState>,
    wakeup: parking_lot::Condvar,
    bot: parking_lot::RwLock<Option<Bot>>,
}

struct SyncState {
    running: bool,
    stats: Statistics,
}

impl BotSynchronizer {
    pub fn new() -> Self {
        BotSynchronizer {
            state: parking_lot::Mutex::new(SyncState {
                running: false,
                stats: Statistics::default(),
            }),
            wakeup: parking_lot::Condvar::new(),
            bot: parking_lot::RwLock::new(None),
        }
    }

    pub fn start(&self, bot: Bot) {
        {
            let mut state = self.state.lock();
            state.running = true;
            state.stats = Statistics::default();
        }
        {
            let mut guard = self.bot.write();
            *guard = Some(bot);
        }
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
    }

    pub fn suggest(&self) -> Option<(Vec<Placement>, MoveInfo)> {
        let guard = self.bot.read();
        let bot = guard.as_ref()?;
        let mut extra_stats = Statistics::default();
        let mut moves = bot.suggest();
        if moves.is_empty() {
            for _ in 0..SUGGEST_WARMUP_ITERS {
                let stats = bot.do_work();
                extra_stats.accumulate(stats);
                moves = bot.suggest();
                if !moves.is_empty() {
                    break;
                }
            }
        }
        drop(guard);

        if extra_stats.nodes > 0 || extra_stats.selections > 0 || extra_stats.expansions > 0 {
            let mut state = self.state.lock();
            state.stats.accumulate(extra_stats);
        }

        let state = self.state.lock();
        let info = MoveInfo {
            nodes: state.stats.nodes,
            nps: 0.0,
            extra: String::new(),
        };
        Some((moves, info))
    }

    pub fn advance(&self, mv: Placement) {
        let mut guard = self.bot.write();
        if let Some(bot) = guard.as_mut() {
            bot.advance(mv);
        }
    }

    pub fn new_piece(&self, piece: PieceKind) {
        let mut guard = self.bot.write();
        if let Some(bot) = guard.as_mut() {
            bot.new_piece(piece);
        }
    }

    pub fn work_loop(&self) {
        loop {
            {
                let mut state = self.state.lock();
                while !state.running {
                    self.wakeup.wait(&mut state);
                }
            }

            let guard = self.bot.read();
            if let Some(bot) = guard.as_ref() {
                let stats = bot.do_work();
                drop(guard);

                let mut state = self.state.lock();
                state.stats.accumulate(stats);
            }
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

    #[test]
    fn suggest_warms_up_new_bot_until_moves_exist() {
        let sync = BotSynchronizer::new();
        let options = BotOptions {
            speculate: true,
            config: Arc::new(BotConfig::default()),
        };
        let bot = Bot::new(
            options,
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
        let (moves, _) = sync.suggest().expect("started bot should answer suggest");

        assert!(
            !moves.is_empty(),
            "suggest should produce at least one move"
        );
    }
}
