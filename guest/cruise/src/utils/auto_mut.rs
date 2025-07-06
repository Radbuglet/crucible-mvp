use std::{
    ops::{Deref, DerefMut},
    rc::Rc,
};

use derive_where::derive_where;

#[derive(Debug, Hash, Eq, PartialEq, Ord, PartialOrd)]
#[derive_where(Clone)]
pub struct AutoMut<T: ?Sized> {
    value: Rc<T>,
}

impl<T: Default> Default for AutoMut<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T: ?Sized> AutoMut<T> {
    pub fn new(value: T) -> Self
    where
        T: Sized,
    {
        Self::wrap(Rc::new(value))
    }

    pub fn wrap(value: Rc<T>) -> Self {
        Self { value }
    }

    pub fn unwrap(self) -> Rc<T> {
        self.value
    }
}

impl<T: ?Sized> Deref for AutoMut<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T: Clone> DerefMut for AutoMut<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        Rc::make_mut(&mut self.value)
    }
}
