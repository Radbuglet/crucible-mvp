use std::marker::PhantomData;

pub struct RendezvousTx<T> {
    _ty: PhantomData<T>,
}

impl<T> RendezvousTx<T> {
    pub fn propose(&mut self, f: impl 'static + FnOnce() -> T) {
        todo!()
    }

    pub fn cancel(&mut self) {
        todo!()
    }
}

pub struct RendezvousRx<T> {
    _ty: PhantomData<T>,
}

impl<T> RendezvousRx<T> {
    pub async fn receive(&self) {
        todo!()
    }
}
