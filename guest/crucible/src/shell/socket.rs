use crucible_abi as abi;
use futures::channel::oneshot;
use thiserror::Error;
use wasmlink::{GuestStrRef, OwnedGuestClosure, bind_port};

use crate::{base::task::wake_executor, net::client::GameSocket};

#[derive(Debug, Clone, Error)]
#[error("{msg}")]
pub struct LoginSocketError {
    msg: String,
}

#[derive(Debug)]
pub struct LoginSocket {
    handle: abi::LoginSocketHandle,
}

impl LoginSocket {
    pub async fn connect(addr: &str) -> Result<Self, LoginSocketError> {
        bind_port! {
            fn [abi::LOGIN_SOCKET_CONNECT] "crucible".login_socket_connect(
                abi::LoginSocketConnectArgs
            );
        }

        let (tx, rx) = oneshot::channel();

        let callback =
            OwnedGuestClosure::<Result<abi::LoginSocketHandle, String>>::new_once(move |res| {
                tx.send(match res.decode() {
                    Ok(handle) => Ok(LoginSocket { handle }),
                    Err(msg) => Err(LoginSocketError { msg: msg.decode() }),
                })
                .unwrap();
                wake_executor();
            });

        login_socket_connect(&abi::LoginSocketConnectArgs {
            addr: GuestStrRef::new(addr),
            callback: callback.handle(),
        });

        rx.await.unwrap()
    }

    pub fn rtt(&self) -> Option<f64> {
        bind_port! {
            fn [abi::LOGIN_SOCKET_GET_RTT] "crucible".login_socket_get_rtt(
                abi::LoginSocketHandle
            ) -> Option<f64>;
        }

        login_socket_get_rtt(&self.handle).decode()
    }

    pub async fn info(&self) -> Result<LoginServerInfo, LoginSocketError> {
        bind_port! {
            fn [abi::LOGIN_SOCKET_GET_INFO] "crucible".login_socket_get_info(
                abi::LoginSocketGetInfoArgs
            );
        }

        let (tx, rx) = oneshot::channel();

        let callback =
            OwnedGuestClosure::<Result<abi::LoginServerInfo, String>>::new_once(move |res| {
                tx.send(match res.decode() {
                    Ok(info) => Ok(LoginServerInfo {
                        motd: info.motd.decode(),
                        content_hash: blake3::Hash::from_bytes(info.content_hash.0),
                        content_server: info.content_server.decode().map(|v| v.decode()),
                    }),
                    Err(msg) => Err(LoginSocketError { msg: msg.decode() }),
                })
                .unwrap();
                wake_executor();
            });

        login_socket_get_info(&abi::LoginSocketGetInfoArgs {
            socket: self.handle,
            callback: callback.handle(),
        });

        rx.await.unwrap()
    }

    pub async fn play(
        &self,
        hash: blake3::Hash,
    ) -> Result<Result<GameSocket, blake3::Hash>, LoginSocketError> {
        bind_port! {
            fn [abi::LOGIN_SOCKET_PLAY] "crucible".login_socket_play(
                abi::LoginSocketPlayArgs
            );
        }

        let (tx, rx) = oneshot::channel();

        let callback = OwnedGuestClosure::<
            Result<Result<abi::GameSocketHandle, abi::ContentHash>, String>,
        >::new_once(move |res| {
            tx.send(match res.decode() {
                Ok(handle) => Ok(match handle.decode() {
                    Ok(handle) => Ok(GameSocket { handle }),
                    Err(hash) => Err(blake3::Hash::from_bytes(hash.0)),
                }),
                Err(msg) => Err(LoginSocketError { msg: msg.decode() }),
            })
            .unwrap();
            wake_executor();
        });

        login_socket_play(&abi::LoginSocketPlayArgs {
            socket: self.handle,
            content_hash: abi::ContentHash(*hash.as_bytes()),
            callback: callback.handle(),
        });

        rx.await.unwrap()
    }
}

impl Drop for LoginSocket {
    fn drop(&mut self) {
        bind_port! {
            fn [abi::LOGIN_SOCKET_CLOSE] "crucible".login_socket_close(
                abi::LoginSocketHandle
            );
        }

        login_socket_close(&self.handle);
    }
}

#[derive(Debug, Clone)]
pub struct LoginServerInfo {
    pub motd: String,
    pub content_hash: blake3::Hash,
    pub content_server: Option<String>,
}
