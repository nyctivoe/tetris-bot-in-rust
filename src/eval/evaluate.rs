use ordered_float::OrderedFloat;
use std::ops::Add;

use crate::data::{GameState, PlacementInfo};
use crate::eval::features::{BoardFeatures, PlacementFeatures};
use crate::eval::weights::Weights;
use tetrisEngine::PieceKind;

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Eval {
    pub value: OrderedFloat<f32>,
}

#[derive(Copy, Clone, Debug)]
pub struct Reward {
    pub value: OrderedFloat<f32>,
}

pub fn evaluate(
    weights: &Weights,
    state: &GameState,
    _info: &PlacementInfo,
    board_feats: &BoardFeatures,
    placement_feats: &PlacementFeatures,
) -> (Eval, Reward) {
    let mut eval = 0.0f32;
    let mut reward = 0.0f32;

    if placement_feats.is_perfect_clear {
        reward += weights.perfect_clear;
    }
    if !placement_feats.is_perfect_clear || !weights.perfect_clear_override {
        if placement_feats.is_back_to_back {
            reward += weights.back_to_back_clear;
        }
        let lines = placement_feats.lines_cleared as usize;
        let clear_idx = lines.min(5);
        if placement_feats.is_spin && !placement_feats.is_mini {
            reward += weights.spin_clears[clear_idx];
        } else if placement_feats.is_spin && placement_feats.is_mini {
            let is_all_spin_piece = matches!(
                placement_feats.placed_kind,
                Some(kind) if kind != PieceKind::T
            );
            if is_all_spin_piece {
                reward += weights.all_mini_clears[clear_idx];
            } else {
                reward += weights.mini_spin_clears[clear_idx];
            }
        } else {
            reward += weights.normal_clears[clear_idx];
        }
        reward += weights.combo_attack * (placement_feats.combo.saturating_sub(1) / 2) as f32;
    }

    if placement_feats.wasted_t {
        reward += weights.wasted_t;
    }
    if placement_feats.wasted_spin_piece {
        reward += weights.wasted_spin_piece;
    }
    reward += weights.softdrop * placement_feats.soft_drop_distance as f32;

    eval += weights.holes * board_feats.holes as f32;
    let coveredness = if weights.max_cell_covered_depth > 0 {
        compute_capped_coveredness(state, weights.max_cell_covered_depth)
    } else {
        board_feats.cell_coveredness
    };
    eval += weights.cell_coveredness * coveredness as f32;

    eval += weights.height * board_feats.max_height as f32;
    if board_feats.max_height > 10 {
        eval += weights.height_upper_half * board_feats.height_above_10 as f32;
    }
    if board_feats.max_height > 15 {
        eval += weights.height_upper_quarter * board_feats.height_above_15 as f32;
    }

    eval += weights.tetris_well_depth * board_feats.tetris_well_depth as f32;
    eval += weights.row_transitions * board_feats.row_transitions as f32;
    eval += weights.col_transitions * board_feats.col_transitions as f32;

    if board_feats.has_back_to_back {
        eval += weights.has_back_to_back;
    }
    eval += weights.b2b_chain_bonus * board_feats.b2b_chain as f32;
    eval += weights.surge_charge_bonus * board_feats.surge_charge as f32;

    for (i, &w) in weights.tslot.iter().enumerate() {
        eval += w * board_feats.tslot[i] as f32;
    }
    for (i, &w) in weights.jslot.iter().enumerate() {
        eval += w * board_feats.jslot[i] as f32;
    }
    for (i, &w) in weights.lslot.iter().enumerate() {
        eval += w * board_feats.lslot[i] as f32;
    }
    for (i, &w) in weights.sslot.iter().enumerate() {
        eval += w * board_feats.sslot[i] as f32;
    }
    for (i, &w) in weights.zslot.iter().enumerate() {
        eval += w * board_feats.zslot[i] as f32;
    }

    (
        Eval { value: eval.into() },
        Reward {
            value: reward.into(),
        },
    )
}

fn compute_capped_coveredness(state: &GameState, max_depth: u32) -> u32 {
    use tetrisEngine::{BOARD_HEIGHT, BOARD_WIDTH};
    let mut total = 0u32;
    let heights = {
        let mut h = [0u32; BOARD_WIDTH];
        for col in 0..BOARD_WIDTH {
            for row in 0..BOARD_HEIGHT {
                if state.board[row * BOARD_WIDTH + col] != 0 {
                    h[col] = (BOARD_HEIGHT - row) as u32;
                    break;
                }
            }
        }
        h
    };
    for col in 0..BOARD_WIDTH {
        let col_h = heights[col];
        if col_h == 0 {
            continue;
        }
        let top_row = BOARD_HEIGHT - col_h as usize;
        for row in (top_row + 1)..BOARD_HEIGHT {
            if state.board[row * BOARD_WIDTH + col] == 0 {
                let depth = (row - top_row) as u32;
                total += depth.min(max_depth);
            }
        }
    }
    total
}

pub trait Evaluation: Ord + Copy + Default + Add<Self::Reward, Output = Self> + 'static {
    type Reward: Copy;
    fn average(of: impl Iterator<Item = Option<Self>>) -> Self;
}

impl Evaluation for Eval {
    type Reward = Reward;

    fn average(of: impl Iterator<Item = Option<Self>>) -> Self {
        let mut count = 0usize;
        let sum: f32 = of
            .map(|v| {
                count += 1;
                v.map(|e| e.value.0).unwrap_or(-1000.0)
            })
            .sum();
        if count == 0 {
            return Eval::default();
        }
        Eval {
            value: (sum / count as f32).into(),
        }
    }
}

impl Add<Reward> for Eval {
    type Output = Self;

    fn add(self, rhs: Reward) -> Eval {
        Eval {
            value: self.value + rhs.value,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eval_default_is_zero() {
        let e = Eval::default();
        assert_eq!(e.value.0, 0.0);
    }

    #[test]
    fn eval_average_of_none_returns_default() {
        let avg = Eval::average(std::iter::empty());
        assert_eq!(avg.value.0, 0.0);
    }

    #[test]
    fn eval_average_of_all_none_returns_minus_1000() {
        let avg = Eval::average(std::iter::once(None::<Eval>));
        assert_eq!(avg.value.0, -1000.0);
    }

    #[test]
    fn eval_add_reward_works() {
        let e = Eval {
            value: OrderedFloat(1.0),
        };
        let r = Reward {
            value: OrderedFloat(2.0),
        };
        let result = e + r;
        assert_eq!(result.value.0, 3.0);
    }

    #[test]
    fn reward_zero_for_no_clear() {
        let weights = Weights::default();
        let state = GameState::empty();
        let info = crate::data::PlacementInfo::default();
        let board_feats = BoardFeatures::default();
        let placement_feats = PlacementFeatures::default();
        let (_, reward) = evaluate(&weights, &state, &info, &board_feats, &placement_feats);
        assert_eq!(reward.value.0, 0.0);
    }
}
