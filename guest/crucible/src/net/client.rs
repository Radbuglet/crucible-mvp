use crucible_abi as abi;
use futures::channel::oneshot;
use thiserror::Error;
use wasmlink::{GuestSliceRef, OwnedGuestClosure, bind_port};

use crate::base::task::wake_executor;

#[derive(Debug, Clone, Error)]
#[error("{msg}")]
pub struct GameSocketError {
    msg: String,
}

#[derive(Debug)]
pub struct GameSocket {
    pub(crate) handle: abi::GameSocketHandle,
}

impl GameSocket {
    pub fn id(&self) -> u64 {
        bind_port! {
            fn [abi::GAME_SOCKET_GET_ID] "crucible".game_socket_get_id(
                abi::GameSocketHandle
            ) -> u64;
        }

        game_socket_get_id(&self.handle)
    }

    pub fn rtt(&self) -> Option<f64> {
        bind_port! {
            fn [abi::GAME_SOCKET_GET_RTT] "crucible".game_socket_get_rtt(
                abi::GameSocketHandle
            ) -> Option<f64>;
        }

        game_socket_get_rtt(&self.handle).decode()
    }

    pub async fn send(&mut self, msg: &[u8]) -> Result<(), GameSocketError> {
        bind_port! {
            fn [abi::GAME_SOCKET_SEND_MSG] "crucible".game_socket_send_msg(
                abi::GameSocketSendMsgArgs
            );

            fn [abi::GAME_SOCKET_CANCEL_SEND_MSG] "crucible".game_socket_cancel_send_msg(
                abi::GameSocketHandle
            );
        }

        let (tx, rx) = oneshot::channel();

        let callback = OwnedGuestClosure::<Result<(), String>>::new_once(move |res| {
            tx.send(
                res.decode()
                    .map_err(|err| GameSocketError { msg: err.decode() }),
            )
            .unwrap();

            wake_executor();
        });

        let guard = scopeguard::guard((), |()| game_socket_cancel_send_msg(&self.handle));

        game_socket_send_msg(&abi::GameSocketSendMsgArgs {
            socket: self.handle,
            message: GuestSliceRef::new(msg),
            callback: callback.handle(),
        });

        let res = rx.await.unwrap();
        scopeguard::ScopeGuard::into_inner(guard);
        res
    }

    pub fn recv(&self) -> Result<Vec<u8>, GameSocketError> {
        bind_port! {
            fn [abi::GAME_SOCKET_RECV_MSG] "crucible".game_socket_recv_msg(
                abi::GameSocketHandle
            ) -> Result<Vec<u8>, String>;
        }

        match game_socket_recv_msg(&self.handle).decode() {
            Ok(msg) => Ok(msg.decode()),
            Err(err) => Err(GameSocketError { msg: err.decode() }),
        }
    }
}

impl Drop for GameSocket {
    fn drop(&mut self) {
        bind_port! {
            fn [abi::GAME_SOCKET_CLOSE] "crucible".game_socket_close(
                abi::GameSocketHandle
            );
        }

        game_socket_close(&self.handle);
    }
}
