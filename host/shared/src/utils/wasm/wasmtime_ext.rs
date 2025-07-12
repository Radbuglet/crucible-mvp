pub trait StoreData: Sized + wasmtime::AsContext {
    fn data(&self) -> &Self::Data;
}

impl<T> StoreData for wasmtime::Store<T> {
    fn data(&self) -> &T {
        self.data()
    }
}

impl<T> StoreData for wasmtime::Caller<'_, T> {
    fn data(&self) -> &T {
        self.data()
    }
}

impl<T> StoreData for wasmtime::StoreContext<'_, T> {
    fn data(&self) -> &T {
        self.data()
    }
}

impl<T> StoreData for wasmtime::StoreContextMut<'_, T> {
    fn data(&self) -> &T {
        self.data()
    }
}

pub trait StoreDataMut: StoreData + wasmtime::AsContextMut {
    fn data_mut(&mut self) -> &mut Self::Data;
}

impl<T> StoreDataMut for wasmtime::Store<T> {
    fn data_mut(&mut self) -> &mut T {
        self.data_mut()
    }
}

impl<T> StoreDataMut for wasmtime::Caller<'_, T> {
    fn data_mut(&mut self) -> &mut T {
        self.data_mut()
    }
}

impl<T> StoreDataMut for wasmtime::StoreContextMut<'_, T> {
    fn data_mut(&mut self) -> &mut T {
        self.data_mut()
    }
}
