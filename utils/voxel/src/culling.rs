use std::{
    fmt,
    ops::{Index, IndexMut, Not},
};

use glam::IVec3;

use crate::utils::{FxHashMap, FxHashSet};

#[derive(Debug)]
pub struct CullingManager {
    /// Maps chunks to a map from source face to faces that can not be seen through that face.
    chunks: FxHashMap<IVec3, FaceMap<FaceMask>>,
}

impl CullingManager {
    pub fn potentially_visible(
        &self,
        start: IVec3,
        allowed_traversals: FaceMask,
    ) -> FxHashSet<IVec3> {
        let mut queue_visited = [(start, allowed_traversals)]
            .into_iter()
            .collect::<FxHashSet<_>>();

        let mut queue = vec![(start, allowed_traversals)];

        while let Some((curr, allowed_traversals)) = queue.pop() {
            for face in allowed_traversals.iter_faces() {
                let neighbor_pos = curr + face.unit();
                let neighbor_cannot_see = self
                    .chunks
                    .get(&neighbor_pos)
                    .unwrap_or(&FaceMap::default())[face];

                let neighbor_allowed_traversals =
                    allowed_traversals & !face.as_mask() & !neighbor_cannot_see;

                if queue_visited.insert((neighbor_pos, neighbor_allowed_traversals)) {
                    queue.push((neighbor_pos, neighbor_allowed_traversals));
                }
            }
        }

        queue_visited.into_iter().map(|v| v.0).collect()
    }
}

#[derive(Copy, Clone, Hash, Eq, PartialEq, Default)]
pub struct FaceMap<T>(pub [T; Face::COUNT]);

impl<T: fmt::Debug> fmt::Debug for FaceMap<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(self.entries()).finish()
    }
}

impl<T> FaceMap<T> {
    pub fn entries(&self) -> impl Iterator<Item = (Face, &T)> {
        Face::VARIANTS.into_iter().zip(&self.0)
    }
}

impl<T> Index<Face> for FaceMap<T> {
    type Output = T;

    fn index(&self, index: Face) -> &Self::Output {
        &self.0[index.as_idx() as usize]
    }
}

impl<T> IndexMut<Face> for FaceMap<T> {
    fn index_mut(&mut self, index: Face) -> &mut Self::Output {
        &mut self.0[index.as_idx() as usize]
    }
}

bitflags::bitflags! {
    #[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Default)]
    pub struct FaceMask: u8 {
        const POS_X = 1 << 0;
        const NEG_X = 1 << 1;
        const POS_Y = 1 << 2;
        const NEG_Y = 1 << 3;
        const POS_Z = 1 << 4;
        const NEG_Z = 1 << 5;
    }
}

impl FaceMask {
    pub fn iter_faces(self) -> impl Iterator<Item = Face> {
        Face::VARIANTS
            .into_iter()
            .filter(move |face| face.as_mask().intersects(self))
    }
}

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
#[repr(u8)]
pub enum Face {
    PosX,
    NegX,
    PosY,
    NegY,
    PosZ,
    NegZ,
}

impl Face {
    pub const COUNT: usize = 6;
    pub const VARIANTS: [Face; Self::COUNT] = [
        Self::PosX,
        Self::NegX,
        Self::PosY,
        Self::NegY,
        Self::PosZ,
        Self::NegZ,
    ];

    pub const MASKS: [FaceMask; Self::COUNT] = {
        let mut arr = [FaceMask::empty(); Self::COUNT];
        let mut i = 0;

        while i < Self::COUNT {
            arr[i] = Self::VARIANTS[i].as_mask();
            i += 1;
        }

        arr
    };

    pub const fn as_idx(self) -> u8 {
        self as u8
    }

    pub const fn from_idx(idx: u8) -> Self {
        Self::VARIANTS[idx as usize]
    }

    pub const fn as_mask(self) -> FaceMask {
        match self {
            Face::PosX => FaceMask::POS_X,
            Face::NegX => FaceMask::NEG_X,
            Face::PosY => FaceMask::POS_Y,
            Face::NegY => FaceMask::NEG_Y,
            Face::PosZ => FaceMask::POS_Z,
            Face::NegZ => FaceMask::NEG_Z,
        }
    }

    pub const fn invert(self) -> Face {
        match self {
            Face::PosX => Face::NegX,
            Face::NegX => Face::PosX,
            Face::PosY => Face::NegY,
            Face::NegY => Face::PosY,
            Face::PosZ => Face::NegZ,
            Face::NegZ => Face::PosZ,
        }
    }

    pub const fn unit(self) -> IVec3 {
        match self {
            Face::PosX => IVec3::X,
            Face::NegX => IVec3::NEG_X,
            Face::PosY => IVec3::Y,
            Face::NegY => IVec3::NEG_Y,
            Face::PosZ => IVec3::Z,
            Face::NegZ => IVec3::NEG_Z,
        }
    }
}

impl Not for Face {
    type Output = Self;

    fn not(self) -> Self::Output {
        self.invert()
    }
}
