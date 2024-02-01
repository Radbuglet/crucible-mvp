pub trait ExtensionFor<T: ?Sized> {}

impl<T: ?Sized> ExtensionFor<T> for T {}

pub trait All {}

impl<T: ?Sized> All for T {}
