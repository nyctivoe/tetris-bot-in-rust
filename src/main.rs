use std::io::{self, BufRead, Write};
use std::sync::Arc;

use tetrisBot::bot::{Bot, BotConfig, BotOptions};
use tetrisBot::data::{BagSet, GameState};
use tetrisBot::sync::BotSynchronizer;
use tetrisBot::tbp::{BoardMessage, BotMessage, FrontendMessage, MoveInfoMessage};

fn main() {
    let config = Arc::new(BotConfig::default());
    let bot_sync = Arc::new(BotSynchronizer::new());

    {
        let bs = bot_sync.clone();
        std::thread::spawn(move || bs.work_loop());
    }

    let info = BotMessage::Info {
        name: "tetrisBot",
        version: env!("CARGO_PKG_VERSION").to_string(),
        author: "nyctivoe",
        features: &[],
    };
    println!("{}", info.to_json());
    io::stdout().flush().unwrap();

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
                let board = board_from_message(&start.board);
                let bag = BagSet::full();
                let state = GameState {
                    board,
                    bag,
                    reserve: start.hold,
                    b2b_chain: if start.back_to_back { 1 } else { 0 },
                    surge_charge: 0,
                    combo: start.combo,
                    combo_active: start.combo != 0,
                    b2b_mode: tetrisEngine::B2BMode::Surge,
                };

                let options = BotOptions {
                    speculate: true,
                    config: config.clone(),
                };

                let bot = Bot::new(options, state, &start.queue);
                bot_sync.start(bot);
            }
            FrontendMessage::Stop => {
                bot_sync.stop();
            }
            FrontendMessage::Suggest => {
                if let Some((moves, info)) = bot_sync.suggest() {
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
            }
            FrontendMessage::Play { mv } => {
                bot_sync.advance(mv.location);
            }
            FrontendMessage::NewPiece { piece } => {
                bot_sync.new_piece(piece);
            }
            FrontendMessage::Quit => break,
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
