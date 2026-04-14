use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;

use tetrisBot::bot::{Bot, BotConfig, BotOptions};
use tetrisBot::data::Placement;
use tetrisBot::data::{BagSet, GameState};
use tetrisBot::sync::BotSynchronizer;
use tetrisBot::tbp::{BoardMessage, BotMessage, FrontendMessage, MoveInfoMessage};

fn main() {
    let config = Arc::new(load_bot_config());
    let worker_count = config.worker_count;
    let bot_sync = Arc::new(BotSynchronizer::new(config.clone()));

    for _ in 0..worker_count {
        let bs = bot_sync.clone();
        std::thread::spawn(move || {
            let gen_cookie = AtomicU64::new(0);
            bs.work_loop(&gen_cookie);
        });
    }

    let info = BotMessage::Info {
        name: "tetrisBot",
        version: env!("CARGO_PKG_VERSION").to_string(),
        author: "nyctivoe",
        features: &[],
    };
    println!("{}", info.to_json());
    io::stdout().flush().unwrap();

    let mut auto_budget_ms: Option<u64> = None;

    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };

        let msg = match FrontendMessage::from_json(&line) {
            Some(m) => m,
            None => continue,
        };

        match msg {
            FrontendMessage::Rules => {
                let ready = BotMessage::Ready;
                println!("{}", ready.to_json());
                io::stdout().flush().unwrap();
            }
            FrontendMessage::Start(start) => {
                auto_budget_ms = start.time_budget_ms;
                let bot = bot_from_start(config.clone(), start);
                prime_bot(&bot, 1);
                bot_sync.start(bot);
                if let Some(budget_ms) = auto_budget_ms {
                    if let Some((moves, info)) = bot_sync.suggest_with_budget(budget_ms) {
                        print_suggestion(moves, info);
                    }
                }
            }
            FrontendMessage::Stop => {
                auto_budget_ms = None;
                bot_sync.stop();
            }
            FrontendMessage::Suggest => {
                if let Some((moves, info)) = bot_sync.suggest() {
                    print_suggestion(moves, info);
                }
            }
            FrontendMessage::Peek => {
                if let Some((moves, info)) = bot_sync.peek() {
                    print_suggestion(moves, info);
                }
            }
            FrontendMessage::Play { mv } => {
                bot_sync.advance(mv.location);
            }
            FrontendMessage::NewPiece { piece } => {
                bot_sync.new_piece(piece);
            }
            FrontendMessage::Advance { mv, new_pieces } => {
                bot_sync.advance_with_pieces(mv.location, new_pieces);
                if let Some(budget_ms) = auto_budget_ms {
                    if let Some((moves, info)) = bot_sync.suggest_with_budget(budget_ms) {
                        print_suggestion(moves, info);
                    }
                }
            }
            FrontendMessage::Quit => {
                bot_sync.shutdown();
                break;
            }
        }
    }
}

fn print_suggestion(moves: Vec<Placement>, info: tetrisBot::sync::MoveInfo) {
    let msg = BotMessage::Suggestion {
        moves,
        move_info: MoveInfoMessage {
            nodes: info.nodes,
            nps: info.nps,
            extra: info.extra,
        },
    };
    println!("{}", msg.to_json());
    io::stdout().flush().unwrap();
}

fn load_bot_config() -> BotConfig {
    let explicit_path = parse_config_path_arg().unwrap_or_else(|err| {
        eprintln!("{err}");
        std::process::exit(2);
    });

    let (config, loaded_from) = BotConfig::load_runtime(explicit_path).unwrap_or_else(|err| {
        eprintln!("{err}");
        std::process::exit(2);
    });

    if let Some(path) = loaded_from {
        eprintln!("loaded bot config from {}", path.display());
    } else {
        eprintln!(
            "no bot config file found; using built-in defaults (set {} or place bot_config.json)",
            tetrisBot::bot::CONFIG_ENV_VAR,
        );
    }

    config
}

fn parse_config_path_arg() -> Result<Option<PathBuf>, String> {
    let mut args = std::env::args_os().skip(1);
    let mut config_path = None;

    while let Some(arg) = args.next() {
        if arg == "--config" {
            let Some(path) = args.next() else {
                return Err("missing path after --config".to_string());
            };
            config_path = Some(PathBuf::from(path));
            continue;
        }

        if let Some(arg) = arg.to_str() {
            if let Some(path) = arg.strip_prefix("--config=") {
                config_path = Some(PathBuf::from(path));
                continue;
            }
        }

        return Err(format!(
            "unknown argument: {:?} (supported: --config <path>)",
            arg
        ));
    }

    Ok(config_path)
}

fn bot_from_start(config: Arc<BotConfig>, start: tetrisBot::tbp::Start) -> Bot {
    let board = board_from_message(&start.board);
    let bag = BagSet::full();
    let b2b_chain = if start.b2b_chain > 0 {
        start.b2b_chain
    } else if start.back_to_back {
        1
    } else {
        0
    };
    let combo_active = start.combo_active || start.combo != 0;

    let state = GameState {
        board,
        bag,
        reserve: start.hold,
        b2b_chain,
        surge_charge: start.surge_charge,
        combo: start.combo,
        combo_active,
        b2b_mode: start.resolved_b2b_mode(),
    };

    let options = BotOptions {
        speculate: config.speculate,
        config,
    };

    Bot::new(options, state, &start.queue)
}

fn prime_bot(bot: &Bot, iterations: usize) {
    for _ in 0..iterations {
        let _ = bot.do_work();
        if !bot.suggest().is_empty() {
            break;
        }
    }
}

fn board_from_message(board: &BoardMessage) -> tetrisEngine::Board {
    use tetrisEngine::{BOARD_HEIGHT, BOARD_WIDTH};
    let mut result = [0i8; BOARD_WIDTH * BOARD_HEIGHT];

    for (y, row) in board.cells.iter().enumerate() {
        if y >= BOARD_HEIGHT {
            break;
        }
        for (x, cell) in row.iter().enumerate() {
            if x >= BOARD_WIDTH {
                break;
            }
            if let Some(_ch) = cell {
                result[y * BOARD_WIDTH + x] = 1;
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use tetrisBot::tbp::Start;
    use tetrisEngine::PieceKind;

    #[test]
    fn start_message_initializes_bot_with_non_empty_suggestions() {
        let start = Start {
            board: BoardMessage {
                cells: vec![vec![None; tetrisEngine::BOARD_WIDTH]; tetrisEngine::BOARD_HEIGHT],
            },
            queue: vec![
                PieceKind::S,
                PieceKind::O,
                PieceKind::T,
                PieceKind::I,
                PieceKind::L,
                PieceKind::Z,
            ],
            hold: None,
            back_to_back: false,
            combo: 0,
            b2b_chain: 0,
            surge_charge: 0,
            combo_active: false,
            b2b_mode: "surge".to_string(),
            time_budget_ms: None,
        };

        let bot = bot_from_start(Arc::new(BotConfig::default()), start);
        let stats = bot.do_work();

        assert!(stats.nodes > 0);
        assert!(!bot.suggest().is_empty());
    }
}
