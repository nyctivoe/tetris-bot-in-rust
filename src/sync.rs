use crate::bot::{Bot, Statistics};
use crate::data::Placement;
use tetrisEngine::PieceKind;

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
        let moves = bot.suggest();
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
