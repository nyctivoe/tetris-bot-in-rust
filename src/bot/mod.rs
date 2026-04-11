pub mod freestyle;

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::data::{GameState, Placement, SearchAction};
use tetrisEngine::PieceKind;

use self::freestyle::Freestyle;

pub struct Bot {
    options: BotOptions,
    current: GameState,
    queue: std::collections::VecDeque<PieceKind>,
    mode: Freestyle,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BotConfig {
    pub freestyle_weights: crate::eval::weights::Weights,
    pub freestyle_exploitation: f64,
}

impl Default for BotConfig {
    fn default() -> Self {
        static DEFAULT: once_cell::sync::Lazy<BotConfig> = once_cell::sync::Lazy::new(|| {
            let weights = crate::eval::weights::Weights::default();
            BotConfig {
                freestyle_exploitation: weights.freestyle_exploitation,
                freestyle_weights: weights,
            }
        });
        DEFAULT.clone()
    }
}

#[derive(Debug)]
pub struct BotOptions {
    pub speculate: bool,
    pub config: Arc<BotConfig>,
}

impl Bot {
    pub fn new(options: BotOptions, root: GameState, queue: &[PieceKind]) -> Self {
        let mode = Freestyle::new(&options, root, queue);
        Bot {
            current: root,
            queue: queue.iter().copied().collect(),
            mode,
            options,
        }
    }

    pub fn advance(&mut self, mv: Placement) {
        if let Some(piece) = self.queue.pop_front() {
            let action = self
                .mode
                .resolve_action(&self.options, mv)
                .unwrap_or_else(|| SearchAction::from_placement(&self.current, piece, mv));
            self.current.advance_action(action);
            if action.consumes_next_piece() && !self.queue.is_empty() {
                self.queue.pop_front();
            }
            self.mode.advance(&self.options, action);
        }
    }

    pub fn new_piece(&mut self, piece: PieceKind) {
        self.queue.push_back(piece);
        self.mode.new_piece(&self.options, piece);
    }

    pub fn suggest(&self) -> Vec<Placement> {
        self.mode.suggest(&self.options)
    }

    pub fn do_work(&self) -> Statistics {
        self.mode.do_work(&self.options)
    }
}

#[derive(Copy, Clone, Debug, Default)]
pub struct Statistics {
    pub nodes: u64,
    pub selections: u64,
    pub expansions: u64,
}

impl Statistics {
    pub fn accumulate(&mut self, other: Self) {
        self.nodes += other.nodes;
        self.selections += other.selections;
        self.expansions += other.expansions;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Placement;

    fn test_options() -> BotOptions {
        BotOptions {
            speculate: true,
            config: Arc::new(BotConfig::default()),
        }
    }

    #[test]
    fn advance_consumes_two_known_queue_pieces_for_empty_hold() {
        let mut bot = Bot::new(
            test_options(),
            GameState::empty(),
            &[PieceKind::T, PieceKind::O, PieceKind::I],
        );

        bot.advance(Placement {
            x: 4,
            y: 37,
            rotation: 0,
            kind: PieceKind::O,
            last_was_rot: false,
            last_rot_dir: None,
            last_kick_idx: None,
            is_spin: false,
            is_mini: false,
        });

        assert_eq!(bot.current.reserve, Some(PieceKind::T));
        assert_eq!(bot.queue.front().copied(), Some(PieceKind::I));
        assert_eq!(bot.queue.len(), 1);
    }

    #[test]
    fn do_work_on_started_bot_produces_moves() {
        let bot = Bot::new(
            test_options(),
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

        let stats = bot.do_work();

        assert!(stats.nodes > 0, "expected expansions from first work step");
        assert!(
            !bot.suggest().is_empty(),
            "expected at least one suggestion"
        );
    }
}
