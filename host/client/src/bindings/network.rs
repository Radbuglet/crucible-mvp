use arid::{Handle, Object as _, Strong, W};
use arid_entity::component;
use crucible_abi as abi;
use wasmlink_wasmtime::{WslLinker, WslLinkerExt};

use crate::{
    services::network::{CertValidationMode, GameSocketHandle},
    utils::{arena::GuestArena, promise::Promise},
};

#[derive(Debug)]
pub struct NetworkBindings {
    endpoint: quinn::Endpoint,
    handles: GuestArena<Strong<GameSocketHandle>>,
}

component!(pub NetworkBindings);

impl NetworkBindingsHandle {
    pub fn new(w: W) -> anyhow::Result<Strong<Self>> {
        let endpoint = quinn::Endpoint::client("0.0.0.0:0".parse().unwrap())?;

        Ok(NetworkBindings {
            endpoint,
            handles: GuestArena::default(),
        }
        .spawn(w))
    }

    pub fn install(self, linker: &mut WslLinker) -> anyhow::Result<()> {
        linker.define_wsl(abi::LOGIN_SOCKET_CONNECT, move |cx, args, ret| {
            let addr = args.addr.read(cx)?.to_string();

            let w = cx.w();

            let callback = Promise::new(move |res| {
                tracing::info!("Connected to remote host: {res:?}");

                // TODO: Invoke callback
            });

            let gs = GameSocketHandle::new(
                self.r(w).endpoint.clone(),
                addr,
                "localhost",
                CertValidationMode::DontAuthenticate,
                callback,
                w,
            );

            ret.finish(cx, &())
        })?;

        Ok(())
    }
}
