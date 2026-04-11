use tetrisEngine::PieceKind;

#[inline]
pub fn kind_index(kind: PieceKind) -> usize {
    match kind {
        PieceKind::I => 0,
        PieceKind::O => 1,
        PieceKind::T => 2,
        PieceKind::S => 3,
        PieceKind::Z => 4,
        PieceKind::J => 5,
        PieceKind::L => 6,
    }
}

#[inline]
pub fn index_to_kind(idx: usize) -> PieceKind {
    match idx {
        0 => PieceKind::I,
        1 => PieceKind::O,
        2 => PieceKind::T,
        3 => PieceKind::S,
        4 => PieceKind::Z,
        5 => PieceKind::J,
        6 => PieceKind::L,
        _ => panic!("invalid piece index"),
    }
}

pub const ALL_KINDS: [PieceKind; 7] = [
    PieceKind::I,
    PieceKind::O,
    PieceKind::T,
    PieceKind::S,
    PieceKind::Z,
    PieceKind::J,
    PieceKind::L,
];

pub struct PieceMap<V> {
    data: [V; 7],
}

impl<V: Default> Default for PieceMap<V> {
    fn default() -> Self {
        PieceMap {
            data: std::array::from_fn(|_| V::default()),
        }
    }
}

impl<V> PieceMap<V> {
    pub fn new(data: [V; 7]) -> Self {
        PieceMap { data }
    }

    #[inline]
    pub fn get(&self, kind: PieceKind) -> &V {
        &self.data[kind_index(kind)]
    }

    #[inline]
    pub fn get_mut(&mut self, kind: PieceKind) -> &mut V {
        &mut self.data[kind_index(kind)]
    }

    pub fn iter(&self) -> impl Iterator<Item = (PieceKind, &V)> {
        ALL_KINDS.iter().copied().zip(self.data.iter())
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (PieceKind, &mut V)> {
        ALL_KINDS.iter().copied().zip(self.data.iter_mut())
    }
}

impl<V> std::ops::Index<PieceKind> for PieceMap<V> {
    type Output = V;
    fn index(&self, index: PieceKind) -> &Self::Output {
        &self.data[kind_index(index)]
    }
}

impl<V> std::ops::IndexMut<PieceKind> for PieceMap<V> {
    fn index_mut(&mut self, index: PieceKind) -> &mut Self::Output {
        &mut self.data[kind_index(index)]
    }
}
