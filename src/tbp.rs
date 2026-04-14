use serde::{Deserialize, Serialize};

use crate::data::Placement;
use tetrisEngine::PieceKind;

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum FrontendMessage {
    #[serde(rename = "rules")]
    Rules,
    #[serde(rename = "start")]
    Start(Start),
    #[serde(rename = "stop")]
    Stop,
    #[serde(rename = "suggest")]
    Suggest,
    #[serde(rename = "peek")]
    Peek,
    #[serde(rename = "play")]
    Play { mv: PlacementMessage },
    #[serde(rename = "new_piece")]
    NewPiece { piece: PieceKind },
    #[serde(rename = "advance")]
    Advance {
        mv: PlacementMessage,
        #[serde(default)]
        new_pieces: Vec<PieceKind>,
    },
    #[serde(rename = "quit")]
    Quit,
}

#[derive(Debug, Deserialize)]
pub struct Start {
    pub board: BoardMessage,
    pub queue: Vec<PieceKind>,
    #[serde(default)]
    pub hold: Option<PieceKind>,
    #[serde(default)]
    pub back_to_back: bool,
    #[serde(default)]
    pub combo: i32,
    #[serde(default)]
    pub b2b_chain: i32,
    #[serde(default)]
    pub surge_charge: i32,
    #[serde(default)]
    pub combo_active: bool,
    #[serde(default = "default_b2b_mode_str")]
    pub b2b_mode: String,
    #[serde(default)]
    pub time_budget_ms: Option<u64>,
}

fn default_b2b_mode_str() -> String {
    "surge".to_string()
}

impl Start {
    pub fn resolved_b2b_mode(&self) -> tetrisEngine::B2BMode {
        match self.b2b_mode.as_str() {
            "chaining" => tetrisEngine::B2BMode::Chaining,
            _ => tetrisEngine::B2BMode::Surge,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct BoardMessage {
    #[serde(default)]
    pub cells: Vec<Vec<Option<String>>>,
}

#[derive(Debug, Deserialize)]
pub struct PlacementMessage {
    pub location: Placement,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum BotMessage {
    #[serde(rename = "info")]
    Info {
        name: &'static str,
        version: String,
        author: &'static str,
        features: &'static [&'static str],
    },
    #[serde(rename = "ready")]
    Ready,
    #[serde(rename = "suggestion")]
    Suggestion {
        moves: Vec<Placement>,
        move_info: MoveInfoMessage,
    },
    #[serde(rename = "error")]
    Error { reason: String },
}

#[derive(Debug, Serialize)]
pub struct MoveInfoMessage {
    pub nodes: u64,
    pub nps: f64,
    pub extra: String,
}

impl FrontendMessage {
    pub fn from_json(line: &str) -> Option<Self> {
        serde_json::from_str(line).ok()
    }
}

impl BotMessage {
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap()
    }
}
