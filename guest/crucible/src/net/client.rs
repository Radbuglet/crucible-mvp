use crucible_abi as abi;
use thiserror::Error;
use wasmlink::{GuestSliceRef, bind_port};

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

    pub fn send(&self, msg: &[u8]) -> Result<(), GameSocketError> {
        bind_port! {
            fn [abi::GAME_SOCKET_SEND_MSG] "crucible".game_socket_send_msg(
                abi::GameSocketSendMsgArgs
            ) -> Result<(), String>;
        }

        game_socket_send_msg(&abi::GameSocketSendMsgArgs {
            socket: self.handle,
            message: GuestSliceRef::new(msg),
        })
        .decode()
        .map_err(|v| GameSocketError { msg: v.decode() })
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
