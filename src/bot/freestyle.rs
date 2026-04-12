use crate::bot::BotOptions;
use crate::dag::{ChildData, Dag};
use crate::data::*;
use crate::eval::evaluate::Eval;
use crate::eval::features::extract_features;
use crate::movegen::find_placements;
use crate::piece_map::PieceMap;
use tetrisEngine::PieceKind;

use super::Statistics;

pub struct Freestyle {
    dag: Dag<Eval>,
}

impl Freestyle {
    pub fn new(_options: &BotOptions, root: GameState, queue: &[PieceKind]) -> Self {
        Freestyle {
            dag: Dag::new(root, queue),
        }
    }

    pub fn advance(&mut self, _options: &BotOptions, action: SearchAction) {
        self.dag.advance(action);
    }

    pub fn new_piece(&mut self, _options: &BotOptions, piece: PieceKind) {
        self.dag.add_piece(piece);
    }

    pub fn suggest(&self, _options: &BotOptions) -> Vec<Placement> {
        self.dag.suggest()
    }

    pub fn resolve_action(
        &self,
        _options: &BotOptions,
        placement: Placement,
    ) -> Option<SearchAction> {
        self.dag.resolve_action(placement)
    }

    pub fn do_work(&self, options: &BotOptions) -> Statistics {
        let mut new_stats = Statistics::default();
        new_stats.selections += 1;

        if let Some(node) = self.dag.select(
            options.speculate,
            options.config.freestyle_weights.freestyle_exploitation,
        ) {
            let (state, next_piece, following_piece) = node.state();
            let next_possibilities = next_piece.map(BagSet::single).unwrap_or(state.bag);

            let mut moves: PieceMap<Vec<(Placement, u32)>> = PieceMap::default();

            {
                let mut pieces_to_gen = next_possibilities;
                if let Some(reserve) = state.reserve {
                    pieces_to_gen.insert(reserve);
                }
                if state.reserve.is_none() {
                    for incoming_piece in next_possibilities.iter() {
                        let hold_empty_possibilities = following_piece
                            .map(BagSet::single)
                            .unwrap_or_else(|| state.bag.after_consuming(incoming_piece));
                        for piece in hold_empty_possibilities.iter() {
                            pieces_to_gen.insert(piece);
                        }
                    }
                }
                for piece in pieces_to_gen.iter() {
                    moves[piece] = find_placements(&state.board, piece);
                }
            }

            let mut children: PieceMap<Vec<ChildData<Eval>>> = PieceMap::default();

            {
                for incoming_piece in next_possibilities.iter() {
                    let hold_empty_possibilities = state.reserve.is_none().then(|| {
                        following_piece
                            .map(BagSet::single)
                            .unwrap_or_else(|| state.bag.after_consuming(incoming_piece))
                    });
                    let hold_empty_moves = hold_empty_possibilities
                        .map(|possibilities| {
                            possibilities
                                .iter()
                                .map(|piece| moves[piece].len())
                                .sum::<usize>()
                        })
                        .unwrap_or(0);
                    let mut actions = Vec::with_capacity(
                        moves[incoming_piece].len()
                            + state
                                .reserve
                                .filter(|&reserve| reserve != incoming_piece)
                                .map(|reserve| moves[reserve].len())
                                .unwrap_or(0)
                            + hold_empty_moves,
                    );

                    actions.extend(moves[incoming_piece].iter().copied().map(
                        |(placement, sd_distance)| {
                            (
                                SearchAction::play_current(incoming_piece, placement),
                                sd_distance,
                            )
                        },
                    ));

                    if let Some(reserve) =
                        state.reserve.filter(|&reserve| reserve != incoming_piece)
                    {
                        actions.extend(moves[reserve].iter().copied().map(
                            |(placement, sd_distance)| {
                                (
                                    SearchAction::use_hold(incoming_piece, placement),
                                    sd_distance,
                                )
                            },
                        ));
                    } else if let Some(hold_empty_possibilities) = hold_empty_possibilities {
                        for piece in hold_empty_possibilities.iter() {
                            actions.extend(moves[piece].iter().copied().map(
                                |(placement, sd_distance)| {
                                    (
                                        SearchAction::hold_empty(incoming_piece, placement),
                                        sd_distance,
                                    )
                                },
                            ));
                        }
                    }

                    for (action, sd_distance) in actions {
                        let mut s = state;
                        let info = s.advance_action(action);

                        let (board_feats, placement_feats) =
                            extract_features(&state, &s, &info, action.placement, sd_distance);

                        let (eval, reward) = crate::eval::evaluate::evaluate(
                            &options.config.freestyle_weights,
                            &s,
                            &info,
                            &board_feats,
                            &placement_feats,
                        );

                        children[incoming_piece].push(ChildData {
                            resulting_state: s,
                            action,
                            eval,
                            reward,
                        });
                    }

                    new_stats.nodes += children[incoming_piece].len() as u64;
                }
            }

            new_stats.expansions += 1;
            node.expand(children);
        }

        new_stats
    }
}
