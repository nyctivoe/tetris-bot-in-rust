use std::cmp::Ordering;

pub mod known;
pub mod speculated;

use once_cell::sync::Lazy;

use crate::data::{GameState, Placement, SearchAction};
use crate::eval::evaluate::Evaluation;
use crate::piece_map::{kind_index, PieceMap};
use tetrisEngine::PieceKind;

pub struct Dag<E: Evaluation> {
    root: GameState,
    top_layer: Box<LayerCommon<E>>,
}

pub struct Selection<'a, E: Evaluation> {
    layers: Vec<&'a LayerCommon<E>>,
    game_state: GameState,
}

pub struct ChildData<E: Evaluation> {
    pub resulting_state: GameState,
    pub action: SearchAction,
    pub eval: E,
    pub reward: E::Reward,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Child<E: Evaluation> {
    pub(super) action: SearchAction,
    pub(super) reward: E::Reward,
    pub(super) cached_eval: E,
}

struct LayerCommon<E: Evaluation> {
    next_layer: Lazy<Box<LayerCommon<E>>>,
    kind: LayerKind<E>,
}

enum LayerKind<E: Evaluation> {
    Known(known::Layer<E>),
    Speculated(speculated::Layer<E>),
}

enum SelectResult {
    Failed,
    Done,
    Advance(SearchAction),
}

struct BackpropUpdate {
    parent: u64,
    speculation_piece: PieceKind,
    action: SearchAction,
    child: u64,
}

impl<E: Evaluation> Dag<E> {
    pub fn new(root: GameState, queue: &[PieceKind]) -> Self {
        let mut top_layer = LayerCommon::new_default();
        top_layer.kind.initialize_root(&root);

        let mut layer = &mut top_layer;
        for &piece in queue {
            layer.kind.despeculate(piece);
            Lazy::force(&layer.next_layer);
            layer = &mut *layer.next_layer;
        }

        Self {
            root,
            top_layer: Box::new(top_layer),
        }
    }

    pub fn advance(&mut self, action: SearchAction) {
        let top_layer = std::mem::replace(&mut *self.top_layer, LayerCommon::new_default());
        self.root.advance_action(action);

        let mut next_layer = Lazy::into_value(top_layer.next_layer).unwrap_or_default();
        for _ in 1..action.queue_consumption() {
            Lazy::force(&next_layer.next_layer);
            next_layer = Lazy::into_value(next_layer.next_layer).unwrap_or_default();
        }

        self.top_layer = next_layer;
        self.top_layer.kind.initialize_root(&self.root);
    }

    pub fn add_piece(&mut self, piece: PieceKind) {
        let mut layer = &mut *self.top_layer;
        loop {
            if layer.kind.despeculate(piece) {
                return;
            }
            Lazy::force(&layer.next_layer);
            layer = &mut *layer.next_layer;
        }
    }

    pub fn suggest(&self) -> Vec<Placement> {
        self.top_layer.kind.suggest(&self.root)
    }

    pub fn resolve_action(&self, placement: Placement) -> Option<SearchAction> {
        self.top_layer.kind.resolve_action(&self.root, placement)
    }

    pub fn select(&self, speculate: bool, exploration: f64) -> Option<Selection<'_, E>> {
        let mut layers = vec![&*self.top_layer];
        let mut game_state = self.root;

        loop {
            let layer = *layers.last()?;
            match layer.kind.select(&game_state, speculate, exploration) {
                SelectResult::Failed => return None,
                SelectResult::Done => return Some(Selection { layers, game_state }),
                SelectResult::Advance(action) => {
                    game_state.advance_action(action);
                    layers.push(layer_after(layer, action.queue_consumption()));
                }
            }
        }
    }
}

impl<E: Evaluation> Selection<'_, E> {
    pub fn state(&self) -> (GameState, Option<PieceKind>, Option<PieceKind>) {
        let current_layer = self.layers.last().copied();
        (
            self.game_state,
            current_layer.and_then(|layer| layer.kind.piece()),
            current_layer.and_then(|layer| layer_after(layer, 1).kind.piece()),
        )
    }

    pub fn expand(self, children: PieceMap<Vec<ChildData<E>>>) {
        let mut layers = self.layers;
        let start_layer = layers.pop().expect("selection must contain a layer");
        let mut next = start_layer
            .kind
            .expand(start_layer, self.game_state, children);

        while let Some(layer) = layers.pop() {
            next = layer.kind.backprop(next, layer);
            if next.is_empty() {
                break;
            }
        }
    }
}

fn update_child<E: Evaluation>(list: &mut [Child<E>], action: SearchAction, child_eval: E) -> bool {
    let Some(mut index) = list.iter().position(|child| child.action == action) else {
        return false;
    };

    list[index].cached_eval = child_eval + list[index].reward;

    if index > 0 && child_order(&list[index], &list[index - 1]) == Ordering::Less {
        let hole = list[index];
        while index > 0 && child_order(&hole, &list[index - 1]) == Ordering::Less {
            list[index] = list[index - 1];
            index -= 1;
        }
        list[index] = hole;
    } else if index + 1 < list.len()
        && child_order(&list[index + 1], &list[index]) == Ordering::Less
    {
        let hole = list[index];
        while index + 1 < list.len() && child_order(&list[index + 1], &hole) == Ordering::Less {
            list[index] = list[index + 1];
            index += 1;
        }
        list[index] = hole;
    }

    index == 0
}

pub(super) fn child_order<E: Evaluation>(left: &Child<E>, right: &Child<E>) -> Ordering {
    right
        .cached_eval
        .cmp(&left.cached_eval)
        .then_with(|| action_order(left.action, right.action))
}

pub(super) fn action_order(left: SearchAction, right: SearchAction) -> Ordering {
    kind_index(left.incoming_kind)
        .cmp(&kind_index(right.incoming_kind))
        .then_with(|| {
            search_action_kind_order(left.kind).cmp(&search_action_kind_order(right.kind))
        })
        .then_with(|| placement_order(left.placement, right.placement))
}

fn search_action_kind_order(kind: crate::data::SearchActionKind) -> u8 {
    match kind {
        crate::data::SearchActionKind::PlayCurrent => 0,
        crate::data::SearchActionKind::UseHold => 1,
        crate::data::SearchActionKind::HoldEmpty => 2,
    }
}

fn placement_order(left: Placement, right: Placement) -> Ordering {
    kind_index(left.kind)
        .cmp(&kind_index(right.kind))
        .then_with(|| left.x.cmp(&right.x))
        .then_with(|| left.y.cmp(&right.y))
        .then_with(|| left.rotation.cmp(&right.rotation))
        .then_with(|| left.last_was_rot.cmp(&right.last_was_rot))
        .then_with(|| left.last_rot_dir.cmp(&right.last_rot_dir))
        .then_with(|| left.last_kick_idx.cmp(&right.last_kick_idx))
        .then_with(|| left.is_spin.cmp(&right.is_spin))
        .then_with(|| left.is_mini.cmp(&right.is_mini))
}

fn layer_after<E: Evaluation>(layer: &LayerCommon<E>, steps: usize) -> &LayerCommon<E> {
    debug_assert!(steps > 0);

    let mut next = &*layer.next_layer;
    for _ in 1..steps {
        next = &next.next_layer;
    }
    next
}

fn create_child_node<E: Evaluation>(
    layer: &LayerCommon<E>,
    child: &ChildData<E>,
    parent: u64,
    speculation_piece: PieceKind,
) -> E {
    match &layer_after(layer, child.action.queue_consumption()).kind {
        LayerKind::Known(next_layer) => next_layer.create_node(child, parent, speculation_piece),
        LayerKind::Speculated(next_layer) => {
            next_layer.create_node(child, parent, speculation_piece)
        }
    }
}

fn child_eval<E: Evaluation>(layer: &LayerCommon<E>, action: SearchAction, raw: u64) -> E {
    layer_after(layer, action.queue_consumption())
        .kind
        .get_eval(raw)
}

impl<E: Evaluation> LayerKind<E> {
    fn initialize_root(&self, root: &GameState) {
        match self {
            Self::Known(layer) => layer.initialize_root(root),
            Self::Speculated(layer) => layer.initialize_root(root),
        }
    }

    fn backprop(
        &self,
        to_update: Vec<BackpropUpdate>,
        layer: &LayerCommon<E>,
    ) -> Vec<BackpropUpdate> {
        match self {
            Self::Known(known_layer) => known_layer.backprop(to_update, layer),
            Self::Speculated(speculated_layer) => speculated_layer.backprop(to_update, layer),
        }
    }

    fn piece(&self) -> Option<PieceKind> {
        match self {
            Self::Known(layer) => Some(layer.piece),
            Self::Speculated(_) => None,
        }
    }

    fn expand(
        &self,
        layer: &LayerCommon<E>,
        parent_state: GameState,
        children: PieceMap<Vec<ChildData<E>>>,
    ) -> Vec<BackpropUpdate> {
        match self {
            Self::Known(known_layer) => known_layer.expand(layer, parent_state, children),
            Self::Speculated(speculated_layer) => {
                speculated_layer.expand(layer, parent_state, children)
            }
        }
    }

    fn select(&self, game_state: &GameState, speculate: bool, exploration: f64) -> SelectResult {
        match self {
            Self::Known(layer) => layer.select(game_state, exploration),
            Self::Speculated(layer) if speculate => layer.select(game_state, exploration),
            Self::Speculated(_) => SelectResult::Failed,
        }
    }

    fn suggest(&self, state: &GameState) -> Vec<Placement> {
        match self {
            Self::Known(layer) => layer.suggest(state),
            Self::Speculated(layer) => layer.suggest(state),
        }
    }

    fn resolve_action(&self, state: &GameState, placement: Placement) -> Option<SearchAction> {
        match self {
            Self::Known(layer) => layer.resolve_action(state, placement),
            Self::Speculated(layer) => layer.resolve_action(state, placement),
        }
    }

    fn despeculate(&mut self, piece: PieceKind) -> bool {
        let old = std::mem::replace(self, LayerKind::Speculated(Default::default()));
        match old {
            LayerKind::Known(layer) => {
                *self = LayerKind::Known(layer);
                false
            }
            LayerKind::Speculated(layer) => {
                let known_layer = known::Layer {
                    states: layer.states.map_values(|node| known::Node {
                        parents: node.parents,
                        eval: node.eval,
                        children: node.children.map(|mut piece_lists| {
                            std::mem::take(&mut piece_lists[kind_index(piece)].children)
                        }),
                        expanding: node.expanding,
                    }),
                    piece,
                };
                *self = LayerKind::Known(known_layer);
                true
            }
        }
    }

    fn get_eval(&self, raw: u64) -> E {
        match self {
            Self::Known(layer) => layer.get_eval(raw),
            Self::Speculated(layer) => layer.get_eval(raw),
        }
    }
}

impl<E: Evaluation> LayerCommon<E> {
    fn new_default() -> Self {
        Self {
            next_layer: Lazy::new(Box::default),
            kind: LayerKind::Speculated(Default::default()),
        }
    }
}

impl<E: Evaluation> Default for LayerCommon<E> {
    fn default() -> Self {
        Self::new_default()
    }
}
