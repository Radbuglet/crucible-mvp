use std::{
    cell::RefCell,
    mem::{self, ManuallyDrop},
};

use futures::{
    executor::{LocalPool, LocalSpawner},
    future::RemoteHandle,
    task::LocalSpawnExt as _,
};

pub extern crate futures;

thread_local! {
    static EXECUTE_SPAWNER: (RefCell<LocalPool>, LocalSpawner) = {
        let pool = LocalPool::new();
        let spawner = pool.spawner();

        (RefCell::new(pool), spawner)
    };
}

pub fn spawn_task<Fut, Ret>(task: Fut) -> TaskHandle<Ret>
where
    Fut: 'static + Future<Output = Ret>,
    Ret: 'static,
{
    let handle = EXECUTE_SPAWNER
        .with(|(_, spawner)| spawner.spawn_local_with_handle(task))
        .expect("spawner already shut down");

    TaskHandle {
        handle: ManuallyDrop::new(handle),
    }
}

pub fn wake_executor() {
    EXECUTE_SPAWNER.with(|(exec, _)| {
        if let Ok(mut guard) = exec.try_borrow_mut() {
            guard.run_until_stalled();
        }
    });
}

#[derive(Debug)]
pub struct TaskHandle<Ret: 'static> {
    handle: ManuallyDrop<RemoteHandle<Ret>>,
}

impl<Ret> TaskHandle<Ret> {
    pub fn cancel(mut self) {
        unsafe { ManuallyDrop::drop(&mut self.handle) };
        mem::forget(self);
    }
}

impl<Ret: 'static> Drop for TaskHandle<Ret> {
    fn drop(&mut self) {
        let handle = unsafe { ManuallyDrop::take(&mut self.handle) };
        handle.forget();
    }
}
