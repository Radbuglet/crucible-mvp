pub trait ExtensionFor<T: ?Sized> {
    fn v(&self) -> &T;

    fn v_mut(&mut self) -> &mut T;
}

impl<T: ?Sized> ExtensionFor<T> for T {
    fn v(&self) -> &T {
        self
    }

    fn v_mut(&mut self) -> &mut T {
        self
    }
}
