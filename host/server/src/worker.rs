use std::sync::Arc;

use anyhow::Context as _;
use crucible_protocol::{
    codec::{
        FrameDecoder, FrameEncoder, feed_packet, recv_packet, send_packet, wrap_stream_rx,
        wrap_stream_tx,
    },
    game,
};
use quinn::{ConnectionError, RecvStream, SendStream};
use tokio::io::AsyncWriteExt;
use tracing::{Instrument as _, info_span};
use wasmall::format::WasmallArchive;

// === Configs === //

#[derive(Debug, Clone)]
pub enum ContentConfig {
    SelfHosted(Arc<WasmallArchive>),
    Content {
        index_hash: blake3::Hash,
        server_url: String,
    },
}

// === GlobalState === //

/// Engine state that can be shared across multiple worker tasks.
#[derive(Debug)]
pub struct GlobalState {
    content: ContentState,
}

#[derive(Debug)]
struct ContentState {
    config: ContentConfig,
    index_hash: blake3::Hash,
}

impl GlobalState {
    pub fn new(content_config: ContentConfig) -> Self {
        Self {
            content: ContentState {
                index_hash: match &content_config {
                    ContentConfig::SelfHosted(archive) => blake3::hash(&archive.index_buf),
                    ContentConfig::Content { index_hash, .. } => *index_hash,
                },
                config: content_config,
            },
        }
    }

    pub async fn listen(self: Arc<Self>, endpoint: quinn::Endpoint) -> anyhow::Result<()> {
        let mut id_gen = 0u64;

        loop {
            let Some(conn) = endpoint.accept().await else {
                break;
            };

            tokio::spawn(
                handle_quinn_net_task(self.clone().process_conn(conn))
                    .instrument(info_span!("connection", id = id_gen)),
            );
            id_gen += 1;
        }

        Ok(())
    }

    async fn process_conn(self: Arc<Self>, conn: quinn::Incoming) -> anyhow::Result<()> {
        tracing::info!(
            "got remote connection from address {}",
            conn.remote_address()
        );

        let conn = conn.accept()?.await?;

        tracing::info!("accepted connection");

        let mut id_gen = 0;

        loop {
            let (tx, rx) = match conn.accept_bi().await {
                Ok(v) => v,
                Err(
                    ConnectionError::ApplicationClosed(_) | ConnectionError::ConnectionClosed(_),
                ) => break,
                Err(e) => return Err(e.into()),
            };

            let tx = wrap_stream_tx(tx);
            let rx = wrap_stream_rx(rx, 64);

            tokio::spawn(
                handle_quinn_net_task(self.clone().process_stream(tx, rx))
                    .instrument(info_span!("stream", id = id_gen)),
            );
            id_gen += 1;
        }

        Ok(())
    }

    async fn process_stream(
        self: Arc<Self>,
        mut tx: FrameEncoder<SendStream>,
        mut rx: FrameDecoder<RecvStream>,
    ) -> anyhow::Result<()> {
        let hello_packet = recv_packet::<game::SbHello1>(&mut rx)
            .await?
            .context("no hello packet sent")?;

        match hello_packet {
            game::SbHello1::Ping => {
                tracing::info!("client wants to ping");

                loop {
                    send_packet(&mut tx, game::CbPingRes).await?;
                    recv_packet::<game::SbPingReq>(&mut rx).await?;
                }
            }
            game::SbHello1::ServerList => {
                tracing::info!("client wants server list");

                send_packet(
                    &mut tx,
                    game::CbServerList1 {
                        motd: "Hello polynyan~".to_string(),
                        icon_png: Vec::new(),
                        content_server: match &self.content.config {
                            ContentConfig::SelfHosted(..) => None,
                            ContentConfig::Content { server_url, .. } => Some(server_url.clone()),
                        },
                        content_hash: self.content.index_hash,
                    },
                )
                .await?;
            }
            game::SbHello1::Download { hash } => 'dl: {
                tracing::info!("client wants to download {hash}");

                let archive = match &self.content.config {
                    ContentConfig::SelfHosted(archive) => archive,
                    ContentConfig::Content { .. } => {
                        tracing::warn!("download not supported");

                        send_packet(&mut tx, game::CbDownloadRes::NotSupported).await?;
                        break 'dl;
                    }
                };

                let content = if hash == self.content.index_hash {
                    &archive.index_buf[..]
                } else if let Some(range) = archive.blobs.get(&hash) {
                    &archive.index_buf[range.clone()]
                } else {
                    tracing::warn!("blob with has {hash} not found");
                    send_packet(&mut tx, game::CbDownloadRes::NotFound).await?;
                    break 'dl;
                };

                feed_packet(
                    &mut tx,
                    game::CbDownloadRes::Found {
                        content_len: content.len() as u32,
                    },
                )
                .await?;

                tx.get_mut().write_all(content).await?;
                tx.get_mut().flush().await?;
            }
            game::SbHello1::PlayChecked { game_hash, id } => {
                let hash_correct = game_hash == self.content.index_hash;

                tracing::info!(
                    "client wants to play game with hash {game_hash:?} ({}) and ID {id:?}",
                    if hash_correct { "correct" } else { "incorrect" }
                );

                if !hash_correct {
                    send_packet(
                        &mut tx,
                        game::CbPlayRes::WrongHash {
                            expected: self.content.index_hash,
                        },
                    )
                    .await?;

                    return Ok(());
                }

                send_packet(&mut tx, game::CbPlayRes::Ready).await?;
            }
            game::SbHello1::PlayUnchecked { id } => {
                tracing::info!("client wants to play game with ID {id:?}");

                send_packet(&mut tx, game::CbPlayRes::Ready).await?;
            }
        }

        tx.get_mut().finish()?;
        tx.get_mut().stopped().await?;

        Ok(())
    }
}

async fn handle_quinn_net_task(f: impl Future<Output = anyhow::Result<()>>) {
    if let Err(err) = f.await {
        match err.downcast_ref::<quinn::ConnectionError>() {
            Some(ConnectionError::ApplicationClosed(_) | ConnectionError::ConnectionClosed(_)) => {
                // (fallthrough)
            }
            _ => {
                tracing::error!("closed erroneously: {err}");
                return;
            }
        }
    }

    tracing::info!("closed gracefully");
}
