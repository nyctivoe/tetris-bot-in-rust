pub mod freestyle;

use std::path::{Path, PathBuf};
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
    #[serde(default = "default_speculate")]
    pub speculate: bool,
    #[serde(default)]
    pub freestyle_weights: crate::eval::weights::Weights,
    #[serde(default = "default_worker_count")]
    pub worker_count: usize,
    #[serde(default = "default_suggest_budget_ms")]
    pub suggest_budget_ms: u64,
    #[serde(default = "default_suggest_min_nodes")]
    pub suggest_min_nodes: u64,
}

pub const CONFIG_ENV_VAR: &str = "TETRIS_BOT_CONFIG";
const DEFAULT_CONFIG_FILE_NAME: &str = "bot_config.json";

fn default_speculate() -> bool {
    true
}

fn default_worker_count() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get().saturating_sub(1).max(1))
        .unwrap_or(1)
}

fn default_suggest_budget_ms() -> u64 {
    500
}

fn default_suggest_min_nodes() -> u64 {
    200
}

impl Default for BotConfig {
    fn default() -> Self {
        static DEFAULT: once_cell::sync::Lazy<BotConfig> = once_cell::sync::Lazy::new(|| {
            serde_json::from_str(include_str!("../../bot_config.json")).unwrap()
        });
        DEFAULT.clone()
    }
}

impl BotConfig {
    pub fn load_from_path(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref();
        let contents = std::fs::read_to_string(path)
            .map_err(|err| format!("failed to read bot config {}: {err}", path.display()))?;
        serde_json::from_str(&contents)
            .map_err(|err| format!("failed to parse bot config {}: {err}", path.display()))
    }

    pub fn load_runtime(explicit_path: Option<PathBuf>) -> Result<(Self, Option<PathBuf>), String> {
        let resolved = Self::resolve_runtime_path(explicit_path)?;
        match resolved {
            Some(path) => Self::load_from_path(&path).map(|config| (config, Some(path))),
            None => Ok((Self::default(), None)),
        }
    }

    pub fn resolve_runtime_path(explicit_path: Option<PathBuf>) -> Result<Option<PathBuf>, String> {
        if let Some(path) = explicit_path {
            return Ok(Some(path));
        }

        if let Some(path) = std::env::var_os(CONFIG_ENV_VAR) {
            if path.is_empty() {
                return Err(format!("{CONFIG_ENV_VAR} is set but empty"));
            }
            return Ok(Some(PathBuf::from(path)));
        }

        Ok(Self::runtime_path_candidates()
            .into_iter()
            .find(|candidate| candidate.is_file()))
    }

    pub fn runtime_path_candidates() -> Vec<PathBuf> {
        let mut candidates = vec![
            PathBuf::from(DEFAULT_CONFIG_FILE_NAME),
            PathBuf::from("tetrisBot").join(DEFAULT_CONFIG_FILE_NAME),
        ];

        if let Ok(current_exe) = std::env::current_exe() {
            if let Some(mut dir) = current_exe.parent().map(Path::to_path_buf) {
                for _ in 0..4 {
                    candidates.push(dir.join(DEFAULT_CONFIG_FILE_NAME));
                    candidates.push(dir.join("tetrisBot").join(DEFAULT_CONFIG_FILE_NAME));
                    if !dir.pop() {
                        break;
                    }
                }
            }
        }

        candidates
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
    pub movegen_calls: u64,
    pub movegen_nanos: u64,
    pub slot_calls: u64,
    pub slot_nanos: u64,
}

impl Statistics {
    pub fn accumulate(&mut self, other: Self) {
        self.nodes += other.nodes;
        self.selections += other.selections;
        self.expansions += other.expansions;
        self.movegen_calls += other.movegen_calls;
        self.movegen_nanos += other.movegen_nanos;
        self.slot_calls += other.slot_calls;
        self.slot_nanos += other.slot_nanos;
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

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
            stats.movegen_calls > 0,
            "expected movegen profiling to record calls"
        );
        assert!(
            stats.slot_calls > 0,
            "expected slot profiling to record calls"
        );
        assert!(
            !bot.suggest().is_empty(),
            "expected at least one suggestion"
        );
    }

    #[test]
    fn bot_config_loads_from_json_file() {
        let mut config = BotConfig::default();
        config.speculate = false;
        config.worker_count = 3;
        config.suggest_budget_ms = 1234;
        config.suggest_min_nodes = 56;
        config.freestyle_weights.softdrop = 1.25;

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("tetrisbot-config-{unique}.json"));

        std::fs::write(
            &path,
            serde_json::to_string(&config).expect("config serialization should succeed"),
        )
        .expect("temp config should be writable");

        let loaded = BotConfig::load_from_path(&path).expect("config should load from json file");
        let _ = std::fs::remove_file(&path);

        assert!(!loaded.speculate);
        assert_eq!(loaded.worker_count, 3);
        assert_eq!(loaded.suggest_budget_ms, 1234);
        assert_eq!(loaded.suggest_min_nodes, 56);
        assert_eq!(loaded.freestyle_weights.softdrop, 1.25);
    }
}
