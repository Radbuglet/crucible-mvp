use futures::channel::oneshot;
use thiserror::Error;
use wasmlink::{GuestStrRef, OwnedGuestClosure, bind_port};

use crate::base::task::wake_executor;

#[derive(Debug, Clone, Error)]
#[error("{msg}")]
pub struct LoginSocketError {
    msg: String,
}

#[derive(Debug)]
pub struct LoginSocket {
    handle: crucible_abi::LoginSocketHandle,
}

impl LoginSocket {
    pub async fn connect(addr: &str) -> Result<Self, LoginSocketError> {
        bind_port! {
            fn [crucible_abi::LOGIN_SOCKET_CONNECT] "crucible".login_socket_connect(
                crucible_abi::LoginSocketConnectArgs
            );
        }

        let (tx, rx) = oneshot::channel();

        let callback =
            OwnedGuestClosure::<Result<crucible_abi::LoginSocketHandle, String>>::new_once(
                move |res| {
                    tx.send(match res.decode() {
                        Ok(handle) => Ok(LoginSocket { handle }),
                        Err(msg) => Err(LoginSocketError { msg: msg.decode() }),
                    })
                    .unwrap();
                    wake_executor();
                },
            );

        login_socket_connect(&crucible_abi::LoginSocketConnectArgs {
            addr: GuestStrRef::new(addr),
            callback: callback.handle(),
        });

        rx.await.unwrap()
    }

    pub fn ping(&self) -> Option<f64> {
        bind_port! {
            fn [crucible_abi::LOGIN_SOCKET_GET_PING] "crucible".login_socket_get_ping(
                crucible_abi::LoginSocketHandle
            ) -> Option<f64>;
        }

        login_socket_get_ping(&self.handle).decode()
    }

    pub async fn info(&self) -> Result<LoginServerInfo, LoginSocketError> {
        bind_port! {
            fn [crucible_abi::LOGIN_SOCKET_GET_INFO] "crucible".login_socket_get_info(
                crucible_abi::LoginSocketGetInfoArgs
            );
        }

        let (tx, rx) = oneshot::channel();

        let callback = OwnedGuestClosure::<Result<crucible_abi::LoginServerInfo, String>>::new_once(
            move |res| {
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
            },
        );

        login_socket_get_info(&crucible_abi::LoginSocketGetInfoArgs {
            socket: self.handle,
            callback: callback.handle(),
        });

        rx.await.unwrap()
    }
}

impl Drop for LoginSocket {
    fn drop(&mut self) {
        bind_port! {
            fn [crucible_abi::LOGIN_SOCKET_CLOSE] "crucible".login_socket_close(
                crucible_abi::LoginSocketHandle
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
