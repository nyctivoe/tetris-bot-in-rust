use std::sync::atomic::{self, AtomicBool};

use rand::prelude::*;

use crate::data::{GameState, Placement, SearchAction};
use crate::eval::evaluate::Evaluation;
use crate::map::StateMap;
use crate::piece_map::PieceMap;
use tetrisEngine::PieceKind;

use super::{
    child_eval, child_order, create_child_node, update_child, BackpropUpdate, Child, ChildData,
    LayerCommon, SelectResult,
};

pub(super) struct Layer<E: Evaluation> {
    pub states: StateMap<Node<E>>,
    pub piece: PieceKind,
}

pub(super) struct Node<E: Evaluation> {
    pub parents: Vec<(u64, SearchAction, PieceKind)>,
    pub eval: E,
    pub children: Option<Vec<Child<E>>>,
    pub expanding: AtomicBool,
}

impl<E: Evaluation> Default for Node<E> {
    fn default() -> Self {
        Self {
            parents: Vec::new(),
            eval: E::default(),
            children: None,
            expanding: AtomicBool::new(false),
        }
    }
}

impl<E: Evaluation> Default for Layer<E> {
    fn default() -> Self {
        Self {
            states: StateMap::new(),
            piece: PieceKind::I,
        }
    }
}

impl<E: Evaluation> Layer<E> {
    pub fn initialize_root(&self, root: &GameState) {
        let _ = self.states.get_or_insert_with(root, Node::default);
    }

    pub fn suggest(&self, state: &GameState) -> Vec<Placement> {
        let Some(node) = self.states.get(state) else {
            return vec![];
        };
        let Some(children) = &node.children else {
            return vec![];
        };

        let mut candidates: Vec<&Child<E>> = children.iter().collect();
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
        children
            .iter()
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

        if children.is_empty() {
            return SelectResult::Failed;
        }

        let sample: f64 = thread_rng().gen();
        let index = ((-sample.ln() / exploration) % children.len() as f64) as usize;
        SelectResult::Advance(children[index].action)
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
        let piece_children = &children[self.piece];
        let parent_index = self.states.index(&parent_state);

        let mut child_list = Vec::with_capacity(piece_children.len());
        for child in piece_children {
            let eval = create_child_node(current_layer, child, parent_index, self.piece);
            child_list.push(Child {
                action: child.action,
                cached_eval: eval + child.reward,
                reward: child.reward,
            });
        }
        child_list.sort_by(child_order);

        let mut parent = self
            .states
            .get_raw_mut(parent_index)
            .expect("known parent state must exist before expansion");
        parent.eval = child_list
            .first()
            .map(|child| child.cached_eval)
            .unwrap_or_default();
        parent.children = Some(child_list);
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
            if update.speculation_piece != self.piece {
                continue;
            }

            let child_eval = child_eval(current_layer, update.action, update.child);
            let Some(mut parent) = self.states.get_raw_mut(update.parent) else {
                continue;
            };
            let Some(children) = &mut parent.children else {
                continue;
            };

            if !update_child(children, update.action, child_eval) {
                continue;
            }

            let eval = children[0].cached_eval;
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
