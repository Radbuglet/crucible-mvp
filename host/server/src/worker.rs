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

            let span = info_span!("connection", id = id_gen);
            id_gen += 1;

            tokio::spawn({
                let me = self.clone();

                async move {
                    tracing::info!(
                        "Got remote connection from address {}!",
                        conn.remote_address()
                    );

                    if let Err(err) = me.process_conn(conn).await {
                        tracing::error!("{err}");
                    }
                }
                .instrument(span)
            });
        }

        Ok(())
    }

    async fn process_conn(self: Arc<Self>, conn: quinn::Incoming) -> anyhow::Result<()> {
        let conn = conn.accept()?.await?;

        tracing::info!("Accepted connection!");

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
                self.clone()
                    .process_stream(tx, rx)
                    .instrument(info_span!("stream", id = id_gen)),
            );
            id_gen += 1;
        }

        tracing::info!("Connection closed!");

        Ok(())
    }

    async fn process_stream(
        self: Arc<Self>,
        tx: FrameEncoder<SendStream>,
        rx: FrameDecoder<RecvStream>,
    ) {
        if let Err(err) = self.process_stream_inner(tx, rx).await {
            match err.downcast_ref::<quinn::ConnectionError>() {
                Some(
                    ConnectionError::ApplicationClosed(_) | ConnectionError::ConnectionClosed(_),
                ) => {
                    // (fallthrough)
                }
                _ => {
                    tracing::error!("stream closed erroneously: {err}")
                }
            }
        }

        tracing::info!("stream closed naturally");
    }

    async fn process_stream_inner(
        self: Arc<Self>,
        mut tx: FrameEncoder<SendStream>,
        mut rx: FrameDecoder<RecvStream>,
    ) -> anyhow::Result<()> {
        let hello_packet = recv_packet::<game::SbHello1>(&mut rx)
            .await?
            .context("no hello packet sent")?;

        match hello_packet {
            game::SbHello1::Ping => loop {
                send_packet(&mut tx, game::CbPingRes).await?;
                recv_packet::<game::SbPingReq>(&mut rx).await?;
            },
            game::SbHello1::ServerList => {
                tracing::info!("Client wants server list");

                send_packet(
                    &mut tx,
                    game::CbServerList1 {
                        motd: "Hello polynyan~".to_string(),
                        icon_png: Vec::new(),
                        content_server: match &self.content.config {
                            ContentConfig::SelfHosted(..) => None,
                            ContentConfig::Content { server_url, .. } => Some(server_url.clone()),
                        },
                        game_hash: self.content.index_hash,
                    },
                )
                .await?;

                tracing::info!("Sent server list!");
            }
            game::SbHello1::Download { hash } => 'dl: {
                let archive = match &self.content.config {
                    ContentConfig::SelfHosted(archive) => archive,
                    ContentConfig::Content { .. } => {
                        send_packet(&mut tx, game::CbDownloadRes::NotSupported).await?;
                        break 'dl;
                    }
                };

                let Some(range) = archive.blobs.get(&hash) else {
                    send_packet(&mut tx, game::CbDownloadRes::NotFound).await?;
                    break 'dl;
                };

                tracing::info!("Client wants to download {hash}");

                feed_packet(
                    &mut tx,
                    game::CbDownloadRes::Found {
                        content_len: range.len() as u32,
                    },
                )
                .await?;

                tx.get_mut()
                    .write_all(&archive.blob_buf[range.clone()])
                    .await?;

                tx.get_mut().flush().await?;
            }
            game::SbHello1::Play { game_hash } => {
                tracing::info!("Client wants to play game with hash {game_hash:?}");

                send_packet(&mut tx, game::CbPlayRes::Ready).await?;
            }
            game::SbHello1::PlayNewStream => todo!(),
        }

        tx.get_mut().finish()?;
        tx.get_mut().stopped().await?;

        Ok(())
    }
}
