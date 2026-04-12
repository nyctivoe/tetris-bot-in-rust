use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Weights {
    pub cell_coveredness: f32,
    pub max_cell_covered_depth: u32,
    pub holes: f32,
    pub row_transitions: f32,
    pub col_transitions: f32,
    pub height: f32,
    pub height_upper_half: f32,
    pub height_upper_quarter: f32,
    pub tetris_well_depth: f32,

    pub has_back_to_back: f32,
    pub b2b_chain_bonus: f32,
    pub surge_charge_bonus: f32,
    pub surge_release_reward: f32,

    pub tslot: [f32; 4],
    pub jslot: [f32; 3],
    pub lslot: [f32; 3],
    pub sslot: [f32; 2],
    pub zslot: [f32; 2],

    pub normal_clears: [f32; 6],
    pub mini_spin_clears: [f32; 6],
    pub spin_clears: [f32; 6],
    pub all_mini_clears: [f32; 6],

    pub back_to_back_clear: f32,
    pub combo_attack: f32,
    pub perfect_clear: f32,
    pub perfect_clear_override: bool,

    pub wasted_t: f32,
    pub wasted_spin_piece: f32,
    pub softdrop: f32,

    pub freestyle_exploitation: f64,
}

impl Default for Weights {
    fn default() -> Self {
        crate::bot::BotConfig::default().freestyle_weights
    }
}
