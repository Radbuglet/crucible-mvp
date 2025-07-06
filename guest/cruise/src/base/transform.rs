use arid::{Component as _, Entity, Handle as _, W, Wr, component, query_removed};
use glam::Affine2;

use crate::utils::auto_mut::AutoMut;

#[derive(Debug)]
pub struct Transform {
    parent: Option<TransformHandle>,
    children: AutoMut<Vec<TransformHandle>>,
    index_in_parent: usize,
    local_transform: Affine2,
    global_transform: Option<Affine2>,
}

component!(Transform);

impl TransformHandle {
    pub fn new(parent: Entity, w: W) -> Self {
        Transform {
            parent: None,
            children: AutoMut::default(),
            index_in_parent: 0,
            local_transform: Affine2::IDENTITY,
            global_transform: Some(Affine2::IDENTITY),
        }
        .spawn(parent, w)
    }

    pub fn local_transform(self, w: Wr) -> Affine2 {
        self.r(w).local_transform
    }

    pub fn set_local_transform(self, new_transform: Affine2, w: W) {
        self.m(w).local_transform = new_transform;
        self.mark_dirty(w);
    }

    pub fn global_transform(self, w: W) -> Affine2 {
        if let Some(cached) = self.r(w).global_transform {
            return cached;
        }

        let Some(parent) = self.parent(w) else {
            self.m(w).global_transform = Some(self.r(w).local_transform);
            return self.r(w).local_transform;
        };

        let new_xf = parent.global_transform(w) * self.local_transform(w);

        self.m(w).global_transform = Some(new_xf);

        new_xf
    }

    pub fn set_global_transform(self, new_transform: Affine2, w: W) {
        let Some(parent) = self.parent(w) else {
            self.set_local_transform(new_transform, w);
            return;
        };

        self.set_local_transform(parent.global_transform(w).inverse() * new_transform, w);
    }

    pub fn map_local_transform(self, f: impl FnOnce(Affine2, W) -> Affine2, w: W) {
        self.set_local_transform(f(self.local_transform(w), w), w);
    }

    pub fn map_global_transform(self, f: impl FnOnce(Affine2, W) -> Affine2, w: W) {
        self.set_global_transform(f(self.global_transform(w), w), w);
    }

    pub fn update_local_transform(self, f: impl FnOnce(&mut Affine2, W), w: W) {
        self.map_local_transform(
            |mut xf, w| {
                f(&mut xf, w);
                xf
            },
            w,
        );
    }

    pub fn update_global_transform(self, f: impl FnOnce(&mut Affine2, W), w: W) {
        self.map_global_transform(
            |mut xf, w| {
                f(&mut xf, w);
                xf
            },
            w,
        );
    }

    pub fn parent(self, w: Wr) -> Option<TransformHandle> {
        self.r(w).parent
    }

    pub fn set_parent(self, new_parent: Option<TransformHandle>, w: W) {
        if new_parent == self.parent(w) {
            return;
        }

        if let Some(old_parent) = self.m(w).parent.take() {
            let index_in_parent = self.r(w).index_in_parent;

            old_parent.m(w).children.swap_remove(index_in_parent);

            if let Some(moved) = old_parent.r(w).children.get(index_in_parent) {
                moved.m(w).index_in_parent = index_in_parent;
            }
        }

        if let Some(new_parent) = new_parent {
            self.m(w).parent = Some(new_parent);

            self.m(w).index_in_parent = new_parent.r(w).children.len();
            new_parent.m(w).children.push(self);
        }

        self.mark_dirty(w);
    }

    pub fn mark_dirty(self, w: W) {
        if self.m(w).global_transform.take().is_none() {
            // Children are already dirty.
            return;
        }

        for &child in &*self.r(w).children.clone() {
            child.mark_dirty(w);
        }
    }
}

pub fn sys_flush_transforms(w: W) {
    for xf in query_removed::<TransformHandle>(w) {
        xf.set_parent(None, w);
    }
}
