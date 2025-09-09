use bytemuck::{Pod, Zeroable};
use wasmlink::{Marshal, PodMarshal, Port, marshal_struct, marshal_tagged_union};

// === Socket === //

pub const LOGIN_SOCKET_CONNECT: Port<LoginSocketConnectArgs> =
    Port::new("crucible", "login_socket_connect");

pub const LOGIN_SOCKET_GET_PING: Port<LoginSocketHandle, Option<f64>> =
    Port::new("crucible", "login_socket_get_ping");

pub const LOGIN_SOCKET_GET_INFO: Port<LoginSocketGetInfoArgs> =
    Port::new("crucible", "login_socket_get_info");

pub const LOGIN_SOCKET_DOWNLOAD: Port<LoginSocketDownloadArgs> =
    Port::new("crucible", "login_socket_download");

pub const LOGIN_SOCKET_CLOSE: Port<LoginSocketHandle> = Port::new("crucible", "login_socket_close");

marshal_struct! {
    pub struct LoginSocketConnectArgs {
        pub addr: String,
        pub callback: fn(Result<LoginSocketHandle, String>),
    }

    pub struct LoginSocketGetInfoArgs {
        pub socket: LoginSocketHandle,
        pub callback: fn(Result<LoginServerInfo, String>),
    }

    pub struct LoginServerInfo {
        pub motd: String,
        pub content_hash: ContentHash,
        pub content_server: Option<String>,
    }

    pub struct LoginSocketDownloadArgs {
        pub socket: LoginSocketHandle,
        pub content_hash: ContentHash,
        pub callback: fn(LoginSocketDownloadEvent),
    }
}

marshal_tagged_union! {
    pub enum LoginSocketDownloadEvent: u16 {
        Finished(()),
        Progress(f64),
        Error(String),
    }
}

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Ord, PartialOrd, Pod, Zeroable)]
#[repr(transparent)]
pub struct LoginSocketHandle {
    pub raw: u32,
}

impl Marshal for LoginSocketHandle {
    type Strategy = PodMarshal<Self>;
}

#[derive(Debug, Copy, Clone, Pod, Zeroable)]
#[repr(transparent)]
pub struct ContentHash(pub [u8; blake3::OUT_LEN]);

impl Marshal for ContentHash {
    type Strategy = PodMarshal<Self>;
}

// === Process === //

pub const SHELL_PROCESS_START: Port<(), ()> = Port::new("crucible", "shell_process_start");

pub const SHELL_PROCESS_SET_PAUSED: Port<(), ()> =
    Port::new("crucible", "shell_process_set_paused");

pub const SHELL_PROCESS_REQUEST_STOP: Port<(), ()> =
    Port::new("crucible", "shell_process_request_stop");

pub const SHELL_PROCESS_STOP: Port<(), ()> = Port::new("crucible", "shell_process_stop");
