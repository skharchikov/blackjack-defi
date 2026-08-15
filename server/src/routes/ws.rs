use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
};
use tokio::sync::mpsc;
use tracing::{error, info, warn};
use ulid::Ulid;

use crate::{
    auth::{AuthPayload, Authenticator, Password},
    protocol::{ClientMessage, ServerMessage},
    session::RequestId,
    AppState,
};

/// Chips granted to a first-time player on their initial login.
const NEW_PLAYER_CHIPS: u32 = 1_000;
use bj_core::domain::{
    engine::command::player::{
        DoubleDown, Hit, JoinTable, LeaveSeat, LeaveTable, PlaceBet, PlayerAction, Stand, TakeSeat,
    },
    engine::snapshot::GameEventDto,
    Seat, TableId,
};

#[utoipa::path(get, path = "/ws", responses((status = 101, description = "WebSocket upgrade")))]
pub async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: AppState) {
    let conn_id = Ulid::new();
    info!("WS connection {conn_id} opened");

    // Auth phase
    let (player_id, authed_username) = loop {
        match socket.recv().await {
            None => {
                info!("conn={conn_id} disconnected before auth");
                return;
            }
            Some(Err(e)) => {
                error!("conn={conn_id} recv error before auth: {e}");
                return;
            }
            Some(Ok(Message::Text(text))) => {
                match serde_json::from_str::<ClientMessage>(&text) {
                    Ok(ClientMessage::Auth { username, password }) => {
                        match state
                            .auth
                            .authenticate(&AuthPayload {
                                username: username.clone(),
                                password: Password::new(password),
                            })
                            .await
                        {
                            Ok(pid) => {
                                // Only seed chips for new players (wallet returns Err if player unknown)
                                if state.wallet.balance(pid).await.is_err() {
                                    let _ = state.wallet.credit(pid, NEW_PLAYER_CHIPS).await;
                                }
                                info!(
                                    "conn={conn_id} authenticated user='{}' player_id={}",
                                    username, pid
                                );
                                if send_msg(
                                    &mut socket,
                                    &ServerMessage::AuthOk {
                                        player_id: pid.to_string(),
                                    },
                                )
                                .await
                                .is_err()
                                {
                                    return;
                                }
                                let balance = state.wallet.balance(pid).await.unwrap_or(0);
                                let _ = send_msg(
                                    &mut socket,
                                    &ServerMessage::Balance { amount: balance },
                                )
                                .await;
                                break (pid, username);
                            }
                            Err(e) => {
                                warn!("conn={conn_id} auth failed user='{}': {e}", username);
                                let _ = send_msg(
                                    &mut socket,
                                    &ServerMessage::AuthError {
                                        reason: e.to_string(),
                                    },
                                )
                                .await;
                                return;
                            }
                        }
                    }
                    _ => {
                        warn!("conn={conn_id} sent non-Auth message before authenticating");
                        let _ = send_msg(
                            &mut socket,
                            &ServerMessage::AuthError {
                                reason: "send Auth first".into(),
                            },
                        )
                        .await;
                    }
                }
            }
            Some(Ok(_)) => {}
        }
    };

    let mut current_table: Option<TableId> = None;
    let (event_fwd_tx, mut event_fwd_rx) = mpsc::channel::<String>(64);
    let mut fwd_abort: Option<tokio::task::JoinHandle<()>> = None;

    loop {
        tokio::select! {
            Some(json) = event_fwd_rx.recv() => {
                if socket.send(Message::Text(json.into())).await.is_err() {
                    error!("conn={conn_id} user='{}' send failed (event forward)", authed_username);
                    break;
                }
            }
            msg = socket.recv() => {
                match msg {
                    None => {
                        info!("conn={conn_id} user='{}' disconnected", authed_username);
                        break;
                    }
                    Some(Err(e)) => {
                        error!("conn={conn_id} user='{}' recv error: {e}", authed_username);
                        break;
                    }
                    Some(Ok(Message::Close(_))) => {
                        info!("conn={conn_id} user='{}' sent close frame", authed_username);
                        break;
                    }
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<ClientMessage>(&text) {
                            Err(e) => {
                                warn!("conn={conn_id} user='{}' parse error: {e}", authed_username);
                                let _ = send_msg(&mut socket, &ServerMessage::CommandError {
                                    request_id: 0,
                                    reason: format!("parse error: {e}"),
                                }).await;
                            }
                            Ok(msg) => {
                                if handle_client_msg(
                                    msg, player_id, &state,
                                    &mut socket, &mut current_table,
                                    &event_fwd_tx, &mut fwd_abort,
                                ).await.is_err() { break; }
                            }
                        }
                    }
                    Some(Ok(_)) => {}
                }
            }
        }
    }

    if let Some(h) = fwd_abort {
        h.abort();
    }
    if let Some(tid) = current_table {
        if let Err(e) = state
            .session
            .send_command(
                tid,
                player_id,
                RequestId(0),
                PlayerAction::LeaveTable(LeaveTable { player_id }),
            )
            .await
        {
            warn!(
                "conn={conn_id} user='{}' leave-table on disconnect failed: {e}",
                authed_username
            );
        }
    }
    info!("conn={conn_id} user='{}' session ended", authed_username);
}

async fn handle_client_msg(
    msg: ClientMessage,
    player_id: bj_core::domain::PlayerId,
    state: &AppState,
    socket: &mut WebSocket,
    current_table: &mut Option<TableId>,
    event_fwd_tx: &mpsc::Sender<String>,
    fwd_abort: &mut Option<tokio::task::JoinHandle<()>>,
) -> Result<(), ()> {
    match msg {
        ClientMessage::JoinTable {
            table_id,
            request_id,
        } => {
            let tid = match table_id.parse::<TableId>() {
                Ok(t) => t,
                Err(_) => {
                    let _ = send_msg(
                        socket,
                        &ServerMessage::CommandError {
                            request_id,
                            reason: "invalid table_id".into(),
                        },
                    )
                    .await;
                    return Ok(());
                }
            };

            // Leave old table before joining a new one
            if let Some(old_tid) = current_table.take() {
                if let Some(h) = fwd_abort.take() {
                    h.abort();
                }
                let _ = state
                    .session
                    .send_command(
                        old_tid,
                        player_id,
                        RequestId(0),
                        PlayerAction::LeaveTable(LeaveTable { player_id }),
                    )
                    .await;
            }

            // Subscribe before joining so PlayerJoined event is never missed.
            let mut rx = match state.session.subscribe(tid).await {
                Ok(rx) => rx,
                Err(e) => {
                    error!("player={player_id} subscribe table={tid} failed: {e}");
                    let _ = send_msg(
                        socket,
                        &ServerMessage::CommandError {
                            request_id,
                            reason: e.to_string(),
                        },
                    )
                    .await;
                    return Ok(());
                }
            };

            // Then join — if rejected, the broadcast receiver is simply dropped.
            if let Err(e) = state
                .session
                .send_command(
                    tid,
                    player_id,
                    RequestId(request_id),
                    PlayerAction::JoinTable(JoinTable { player_id }),
                )
                .await
            {
                warn!("player={player_id} join table={tid} rejected: {e}");
                let _ = send_msg(
                    socket,
                    &ServerMessage::CommandError {
                        request_id,
                        reason: e.to_string(),
                    },
                )
                .await;
                return Ok(());
            }
            info!("player={player_id} joined table={tid}");

            // Snapshot — on failure, undo the join so the actor state stays consistent.
            match state.session.snapshot(tid, player_id).await {
                Ok(snap) => {
                    let _ = send_msg(
                        socket,
                        &ServerMessage::Snapshot {
                            table_id: tid.to_string(),
                            state: snap,
                        },
                    )
                    .await;
                }
                Err(e) => {
                    error!("player={player_id} snapshot table={tid} failed: {e}");
                    let _ = state
                        .session
                        .send_command(
                            tid,
                            player_id,
                            RequestId(0),
                            PlayerAction::LeaveTable(LeaveTable { player_id }),
                        )
                        .await;
                    let _ = send_msg(
                        socket,
                        &ServerMessage::CommandError {
                            request_id,
                            reason: e.to_string(),
                        },
                    )
                    .await;
                    return Ok(());
                }
            }

            // Start forwarder with the already-subscribed receiver
            if let Some(h) = fwd_abort.take() {
                h.abort();
            }
            let tx = event_fwd_tx.clone();
            let tid_str = tid.to_string();
            let wallet = state.wallet.clone();
            let handle = tokio::spawn(async move {
                loop {
                    match rx.recv().await {
                        Ok(event) => {
                            let is_finished = matches!(
                                event.payload,
                                bj_core::domain::engine::event::payload::EventPayload::GameFinished { .. }
                            );
                            let dto = GameEventDto {
                                game_id: event.game_id,
                                seq: event.event_seq_id.0,
                                payload: event.payload,
                            };
                            let msg = ServerMessage::Event {
                                table_id: tid_str.clone(),
                                event: dto,
                            };
                            if let Ok(json) = serde_json::to_string(&msg) {
                                if tx.send(json).await.is_err() {
                                    break;
                                }
                            }
                            if is_finished {
                                let balance = wallet.balance(player_id).await.unwrap_or(0);
                                if let Ok(json) = serde_json::to_string(&ServerMessage::Balance {
                                    amount: balance,
                                }) {
                                    let _ = tx.send(json).await;
                                }
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            warn!("table={tid_str} event forwarder lagged by {n} messages");
                            let err = ServerMessage::CommandError {
                                request_id: 0,
                                reason: "event stream lagged, please rejoin".into(),
                            };
                            if let Ok(json) = serde_json::to_string(&err) {
                                let _ = tx.send(json).await;
                            }
                            break;
                        }
                        Err(e) => {
                            error!("table={tid_str} event forwarder recv error: {e}");
                            break;
                        }
                    }
                }
            });
            *fwd_abort = Some(handle);
            *current_table = Some(tid);
            let _ = send_msg(socket, &ServerMessage::CommandAck { request_id }).await;
        }

        ClientMessage::LeaveTable {
            table_id,
            request_id,
        } => {
            let tid = match table_id.parse::<TableId>() {
                Ok(t) => t,
                Err(_) => {
                    let _ = send_msg(
                        socket,
                        &ServerMessage::CommandError {
                            request_id,
                            reason: "invalid table_id".into(),
                        },
                    )
                    .await;
                    return Ok(());
                }
            };
            if let Err(e) = state
                .session
                .send_command(
                    tid,
                    player_id,
                    RequestId(request_id),
                    PlayerAction::LeaveTable(LeaveTable { player_id }),
                )
                .await
            {
                warn!("player={player_id} leave table={tid} failed: {e}");
                let _ = send_msg(
                    socket,
                    &ServerMessage::CommandError {
                        request_id,
                        reason: e.to_string(),
                    },
                )
                .await;
            } else {
                info!("player={player_id} left table={tid}");
                if let Some(h) = fwd_abort.take() {
                    h.abort();
                }
                *current_table = None;
                let _ = send_msg(socket, &ServerMessage::CommandAck { request_id }).await;
            }
        }

        ClientMessage::PlaceBet {
            table_id,
            request_id,
            amount,
        } => {
            send_player_cmd(
                socket,
                state,
                player_id,
                &table_id,
                request_id,
                PlayerAction::PlaceBet(PlaceBet { player_id, amount }),
            )
            .await?;
        }
        ClientMessage::Hit {
            table_id,
            request_id,
        } => {
            send_player_cmd(
                socket,
                state,
                player_id,
                &table_id,
                request_id,
                PlayerAction::Hit(Hit { player_id }),
            )
            .await?;
        }
        ClientMessage::Stand {
            table_id,
            request_id,
        } => {
            send_player_cmd(
                socket,
                state,
                player_id,
                &table_id,
                request_id,
                PlayerAction::Stand(Stand { player_id }),
            )
            .await?;
        }
        ClientMessage::DoubleDown {
            table_id,
            request_id,
        } => {
            send_player_cmd(
                socket,
                state,
                player_id,
                &table_id,
                request_id,
                PlayerAction::DoubleDown(DoubleDown { player_id }),
            )
            .await?;
        }

        ClientMessage::LeaveSeat {
            table_id,
            request_id,
        } => {
            send_player_cmd(
                socket,
                state,
                player_id,
                &table_id,
                request_id,
                PlayerAction::LeaveSeat(LeaveSeat { player_id }),
            )
            .await?;
        }

        ClientMessage::TakeSeat {
            table_id,
            request_id,
            seat,
        } => {
            let seat = match seat {
                Some(n) => match Seat::try_from(n) {
                    Ok(s) => Some(s),
                    Err(_) => {
                        let _ = send_msg(
                            socket,
                            &ServerMessage::CommandError {
                                request_id,
                                reason: format!("invalid seat number {n}: must be 1–7"),
                            },
                        )
                        .await;
                        return Ok(());
                    }
                },
                None => None,
            };
            send_player_cmd(
                socket,
                state,
                player_id,
                &table_id,
                request_id,
                PlayerAction::TakeSeat(TakeSeat { player_id, seat }),
            )
            .await?;
        }

        ClientMessage::DealerOpenBetting { request_id, .. }
        | ClientMessage::DealerDealCards { request_id, .. }
        | ClientMessage::DealerPlayHand { request_id, .. }
        | ClientMessage::DealerSettle { request_id, .. } => {
            let _ = send_msg(
                socket,
                &ServerMessage::CommandError {
                    request_id,
                    reason: "dealer commands not supported via WS in this version".into(),
                },
            )
            .await;
        }

        ClientMessage::Auth { .. } => {
            let _ = send_msg(
                socket,
                &ServerMessage::CommandError {
                    request_id: 0,
                    reason: "already authenticated".into(),
                },
            )
            .await;
        }
    }
    Ok(())
}

async fn send_player_cmd(
    socket: &mut WebSocket,
    state: &AppState,
    player_id: bj_core::domain::PlayerId,
    table_id_str: &str,
    request_id: u64,
    action: PlayerAction,
) -> Result<(), ()> {
    match table_id_str.parse::<TableId>() {
        Err(_) => {
            let _ = send_msg(
                socket,
                &ServerMessage::CommandError {
                    request_id,
                    reason: "invalid table_id".into(),
                },
            )
            .await;
        }
        Ok(tid) => {
            match state
                .session
                .send_command(tid, player_id, RequestId(request_id), action)
                .await
            {
                Ok(_) => {
                    let _ = send_msg(socket, &ServerMessage::CommandAck { request_id }).await;
                }
                Err(e) => {
                    warn!("player={player_id} command rejected on table={tid}: {e}");
                    let _ = send_msg(
                        socket,
                        &ServerMessage::CommandError {
                            request_id,
                            reason: e.to_string(),
                        },
                    )
                    .await;
                }
            }
        }
    }
    Ok(())
}

async fn send_msg(socket: &mut WebSocket, msg: &ServerMessage) -> Result<(), ()> {
    let json = serde_json::to_string(msg).map_err(|_| ())?;
    socket
        .send(Message::Text(json.into()))
        .await
        .map_err(|_| ())
}
