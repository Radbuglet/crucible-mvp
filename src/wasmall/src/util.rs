// === ExtensionFor === //

mod extension_for {
    pub trait Sealed<T: ?Sized> {}
}

pub trait ExtensionFor<T: ?Sized>: extension_for::Sealed<T> {
    fn v(&self) -> &T;

    fn v_mut(&mut self) -> &mut T;
}

impl<T: ?Sized> extension_for::Sealed<T> for T {}

impl<T: ?Sized> ExtensionFor<T> for T {
    fn v(&self) -> &T {
        self
    }

    fn v_mut(&mut self) -> &mut T {
        self
    }
}

// === VecExt === //

pub trait VecExt<T>: ExtensionFor<Vec<T>> {
    fn ensure_length(&mut self, len: usize)
    where
        T: Default,
    {
        if self.v_mut().len() < len {
            self.v_mut().resize_with(len, Default::default);
        }
    }

    fn ensure_index(&mut self, index: usize) -> &mut T
    where
        T: Default,
    {
        self.ensure_length(index + 1);
        &mut self.v_mut()[index]
    }
}

impl<T> VecExt<T> for Vec<T> {}

pub trait SliceExt<T>: ExtensionFor<[T]> {
    fn limit_len(&self, len: usize) -> &[T] {
        &self.v()[..self.v().len().min(len)]
    }

	fn to_array<const N: usize>(&self) -> [T; N]
	where
		T: Copy,
	{
		std::array::from_fn(|i| self.v()[i])
	}
}

impl<T> SliceExt<T> for [T] {}
