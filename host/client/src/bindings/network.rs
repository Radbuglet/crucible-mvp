use arid::{Handle, Strong, W};
use arid_entity::{Component, EntityHandle, component};
use crucible_abi as abi;
use wasmlink_wasmtime::{WslLinker, WslLinkerExt, WslStoreExt};

use crate::{
    app::App,
    services::network::{CertValidationMode, GameSocket, LoginSocket},
    utils::{arena::GuestArena, winit::BackgroundTasks},
};

#[derive(Debug)]
pub struct NetworkBindings {
    endpoint: quinn::Endpoint,
    login_sockets: GuestArena<LoginSocket>,
    game_sockets: GuestArena<GameSocket>,
    background: BackgroundTasks<App>,
}

component!(pub NetworkBindings);

impl NetworkBindingsHandle {
    pub fn new(
        owner: EntityHandle,
        background: BackgroundTasks<App>,
        w: W,
    ) -> anyhow::Result<Strong<Self>> {
        let endpoint = quinn::Endpoint::client("0.0.0.0:0".parse().unwrap())?;

        Ok(NetworkBindings {
            endpoint,
            login_sockets: GuestArena::default(),
            game_sockets: GuestArena::default(),
            background,
        }
        .attach(owner, w))
    }

    pub fn install(self, linker: &mut WslLinker) -> anyhow::Result<()> {
        linker.define_wsl(abi::LOGIN_SOCKET_CONNECT, move |cx, args, ret| {
            let addr = args.addr.read(cx)?.to_string();

            let w = cx.w();

            let socket = LoginSocket::new(
                self.r(w).background.clone(),
                self.r(w).endpoint.clone(),
                addr,
                "localhost",
                CertValidationMode::DontAuthenticate,
            );

            self.r(w)
                .background
                .spawn_responder(socket, move |_event_loop, app, res| {
                    let w = &mut app.world;

                    let init = app.init.as_mut().unwrap();

                    let socket = match res {
                        Ok(v) => v,
                        Err(err) => {
                            init.store.run_wsl_root(&mut app.world, |cx| {
                                args.callback.call(cx, &Err(&err.to_string()))
                            })?;

                            return Ok(());
                        }
                    };

                    let handle = self.m(w).login_sockets.add(socket)?;

                    init.store.run_wsl_root(&mut app.world, |cx| {
                        args.callback
                            .call(cx, &Ok(abi::LoginSocketHandle { raw: handle }))
                    })?;

                    Ok(())
                });

            ret.finish(cx, &())
        })?;

        linker.define_wsl(abi::LOGIN_SOCKET_GET_INFO, move |cx, args, ret| {
            let w = cx.w();

            self.r(w).background.spawn_responder(
                self.r(w).login_sockets.get(args.socket.raw)?.info(),
                move |_event_loop, app, res| {
                    let init = app.init.as_mut().unwrap();

                    init.store.run_wsl_root(&mut app.world, |cx| match res {
                        Ok(info) => args.callback.call(
                            cx,
                            &Ok(abi::LoginServerInfo {
                                motd: &info.motd,
                                content_hash: abi::ContentHash(*info.content_hash.as_bytes()),
                                content_server: info.content_server.as_deref(),
                            }),
                        ),
                        Err(err) => args.callback.call(cx, &Err(&err.to_string())),
                    })?;

                    Ok(())
                },
            );

            ret.finish(cx, &())
        })?;

        linker.define_wsl(abi::LOGIN_SOCKET_GET_RTT, move |cx, args, ret| {
            let w = cx.w();

            let rtt = self.r(w).login_sockets.get(args.raw)?.rtt();

            ret.finish(cx, &rtt)
        })?;

        linker.define_wsl(abi::LOGIN_SOCKET_CLOSE, move |cx, args, ret| {
            _ = self.m(cx.w()).login_sockets.remove(args.raw)?;

            ret.finish(cx, &())
        })?;

        linker.define_wsl(abi::LOGIN_SOCKET_PLAY, move |cx, args, ret| {
            let w = cx.w();

            self.r(w).background.spawn_responder(
                self.r(w)
                    .login_sockets
                    .get(args.socket.raw)?
                    .play(blake3::Hash::from_bytes(args.content_hash.0)),
                move |_event_loop, app, res| {
                    let init = app.init.as_mut().unwrap();

                    init.store.run_wsl_root::<anyhow::Result<()>>(
                        &mut app.world,
                        |cx| match res {
                            Ok(Ok(socket)) => {
                                let socket = self.m(cx.w()).game_sockets.add(socket)?;

                                args.callback
                                    .call(cx, &Ok(Ok(abi::GameSocketHandle { raw: socket })))?;

                                Ok(())
                            }
                            Ok(Err(content_hash)) => {
                                args.callback.call(
                                    cx,
                                    &Ok(Err(abi::ContentHash(*content_hash.as_bytes()))),
                                )?;

                                Ok(())
                            }
                            Err(err) => {
                                args.callback.call(cx, &Err(&err.to_string()))?;

                                Ok(())
                            }
                        },
                    )?;

                    Ok(())
                },
            );

            ret.finish(cx, &())
        })?;

        linker.define_wsl(abi::GAME_SOCKET_CLOSE, move |cx, args, ret| {
            self.m(cx.w()).game_sockets.remove(args.raw)?;

            ret.finish(cx, &())
        })?;

        Ok(())
    }
}
