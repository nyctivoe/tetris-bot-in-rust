use std::sync::atomic::{self, AtomicBool};

use rand::prelude::*;

use crate::data::{BagSet, GameState, Placement, SearchAction};
use crate::eval::evaluate::Evaluation;
use crate::map::StateMap;
use crate::piece_map::{kind_index, PieceMap, ALL_KINDS};
use tetrisEngine::PieceKind;

use super::{
    child_eval, child_order, create_child_node, update_child, BackpropUpdate, Child, ChildData,
    LayerCommon, SelectResult,
};

pub(super) struct Layer<E: Evaluation> {
    pub states: StateMap<Node<E>>,
}

pub(super) struct Node<E: Evaluation> {
    pub parents: Vec<(u64, SearchAction, PieceKind)>,
    pub eval: E,
    pub children: Option<Vec<PieceChild<E>>>,
    pub expanding: AtomicBool,
    pub bag: BagSet,
}

#[derive(Clone)]
pub(super) struct PieceChild<E: Evaluation> {
    pub children: Vec<Child<E>>,
}

impl<E: Evaluation> Default for Node<E> {
    fn default() -> Self {
        Self {
            parents: Vec::new(),
            eval: E::default(),
            children: None,
            expanding: AtomicBool::new(false),
            bag: BagSet::full(),
        }
    }
}

impl<E: Evaluation> Default for Layer<E> {
    fn default() -> Self {
        Self {
            states: StateMap::new(),
        }
    }
}

impl<E: Evaluation> Layer<E> {
    pub fn initialize_root(&self, root: &GameState) {
        let _ = self.states.get_or_insert_with(root, || Node {
            bag: root.bag,
            ..Node::default()
        });
    }

    pub fn suggest(&self, state: &GameState) -> Vec<Placement> {
        let Some(node) = self.states.get(state) else {
            return vec![];
        };
        let Some(children) = &node.children else {
            return vec![];
        };

        let mut candidates: Vec<&Child<E>> = Vec::new();
        for piece in state.bag.iter() {
            if let Some(best) = children
                .get(kind_index(piece))
                .and_then(|piece_children| piece_children.children.first())
            {
                candidates.push(best);
            }
        }
        candidates.sort_by(|a, b| child_order(a, b));
        let mut seen = std::collections::HashSet::new();
        candidates
            .into_iter()
            .map(|child| child.action.placement)
            .filter(|placement| seen.insert(*placement))
            .collect()
    }

    pub fn resolve_action(&self, state: &GameState, placement: Placement) -> Option<SearchAction> {
        let node = self.states.get(state)?;
        let children = node.children.as_ref()?;

        state
            .bag
            .iter()
            .filter_map(|piece| children.get(kind_index(piece)))
            .flat_map(|piece_children| piece_children.children.iter())
            .find(|child| child.action.placement == placement)
            .map(|child| child.action)
    }

    pub fn select(&self, game_state: &GameState, exploration: f64) -> SelectResult {
        let Some(node) = self.states.get(game_state) else {
            return SelectResult::Failed;
        };

        let children = match &node.children {
            None => {
                if node.expanding.swap(true, atomic::Ordering::Relaxed) {
                    return SelectResult::Failed;
                }
                return SelectResult::Done;
            }
            Some(children) => children,
        };

        let bag = game_state.bag;
        if bag.is_empty() {
            return SelectResult::Failed;
        }

        let next = bag
            .iter()
            .nth(thread_rng().gen_range(0..bag.len()))
            .expect("non-empty bag must yield a piece");
        let list = match children.get(kind_index(next)) {
            Some(piece_children) if !piece_children.children.is_empty() => &piece_children.children,
            _ => return SelectResult::Failed,
        };

        let sample: f64 = thread_rng().gen();
        let index = ((-sample.ln() / exploration) % list.len() as f64) as usize;
        SelectResult::Advance(list[index].action)
    }

    pub fn get_eval(&self, raw: u64) -> E {
        self.states
            .get_raw(raw)
            .map(|node| node.eval)
            .unwrap_or_default()
    }

    pub fn create_node(
        &self,
        child: &ChildData<E>,
        parent: u64,
        speculation_piece: PieceKind,
    ) -> E {
        let mut node = self
            .states
            .get_or_insert_with(&child.resulting_state, || Node {
                eval: child.eval,
                bag: child.resulting_state.bag,
                ..Node::default()
            });
        let link = (parent, child.action, speculation_piece);
        if !node.parents.contains(&link) {
            node.parents.push(link);
        }
        node.eval
    }

    pub fn expand(
        &self,
        current_layer: &LayerCommon<E>,
        parent_state: GameState,
        children: PieceMap<Vec<ChildData<E>>>,
    ) -> Vec<BackpropUpdate> {
        let parent_index = self.states.index(&parent_state);
        let mut piece_children = vec![
            PieceChild {
                children: Vec::new()
            };
            ALL_KINDS.len()
        ];

        for &piece in &ALL_KINDS {
            let piece_data = &children[piece];
            let mut child_list = Vec::with_capacity(piece_data.len());
            for child in piece_data {
                let eval = create_child_node(current_layer, child, parent_index, piece);
                child_list.push(Child {
                    action: child.action,
                    cached_eval: eval + child.reward,
                    reward: child.reward,
                });
            }
            child_list.sort_by(child_order);
            piece_children[kind_index(piece)] = PieceChild {
                children: child_list,
            };
        }

        let mut parent = self
            .states
            .get_raw_mut(parent_index)
            .expect("speculated parent state must exist before expansion");
        let parent_bag = parent.bag;
        parent.eval = E::average(parent_bag.iter().map(|piece| {
            piece_children
                .get(kind_index(piece))
                .and_then(|piece_children| piece_children.children.first())
                .map(|child| child.cached_eval)
        }));
        parent.children = Some(piece_children);
        parent.expanding.store(false, atomic::Ordering::Relaxed);

        parent
            .parents
            .iter()
            .copied()
            .map(|(grandparent, action, speculation_piece)| BackpropUpdate {
                parent: grandparent,
                action,
                speculation_piece,
                child: parent_index,
            })
            .collect()
    }

    pub fn backprop(
        &self,
        to_update: Vec<BackpropUpdate>,
        current_layer: &LayerCommon<E>,
    ) -> Vec<BackpropUpdate> {
        let mut updates = Vec::new();

        for update in to_update {
            let child_eval = child_eval(current_layer, update.action, update.child);
            let Some(mut parent) = self.states.get_raw_mut(update.parent) else {
                continue;
            };
            let parent_bag = parent.bag;
            let Some(children) = &mut parent.children else {
                continue;
            };
            let Some(list) = children.get_mut(kind_index(update.speculation_piece)) else {
                continue;
            };

            if !update_child(&mut list.children, update.action, child_eval) {
                continue;
            }

            let eval = E::average(parent_bag.iter().map(|piece| {
                children
                    .get(kind_index(piece))
                    .and_then(|piece_children| piece_children.children.first())
                    .map(|child| child.cached_eval)
            }));
            if parent.eval == eval {
                continue;
            }

            parent.eval = eval;
            updates.extend(parent.parents.iter().copied().map(
                |(grandparent, action, speculation_piece)| BackpropUpdate {
                    parent: grandparent,
                    action,
                    speculation_piece,
                    child: update.parent,
                },
            ));
        }

        updates
    }
}
