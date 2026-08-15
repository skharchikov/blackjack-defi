use crate::session::summary::TableSummary;
use bj_core::domain::engine::snapshot::{GameEventDto, GameStateSnapshot};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    Auth {
        username: String,
        password: String,
    },
    JoinTable {
        table_id: String,
        request_id: u64,
    },
    LeaveSeat {
        table_id: String,
        request_id: u64,
    },
    LeaveTable {
        table_id: String,
        request_id: u64,
    },
    PlaceBet {
        table_id: String,
        request_id: u64,
        amount: u32,
    },
    Hit {
        table_id: String,
        request_id: u64,
    },
    Stand {
        table_id: String,
        request_id: u64,
    },
    DoubleDown {
        table_id: String,
        request_id: u64,
    },
    TakeSeat {
        table_id: String,
        request_id: u64,
        #[serde(default)]
        seat: Option<u8>,
    },
    DealerOpenBetting {
        table_id: String,
        request_id: u64,
    },
    DealerDealCards {
        table_id: String,
        request_id: u64,
    },
    DealerPlayHand {
        table_id: String,
        request_id: u64,
    },
    DealerSettle {
        table_id: String,
        request_id: u64,
    },
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum ServerMessage {
    AuthOk {
        player_id: String,
    },
    AuthError {
        reason: String,
    },
    TableList {
        tables: Vec<TableSummary>,
    },
    Snapshot {
        table_id: String,
        state: GameStateSnapshot,
    },
    Event {
        table_id: String,
        event: GameEventDto,
    },
    CommandAck {
        request_id: u64,
    },
    CommandError {
        request_id: u64,
        reason: String,
    },
    Balance {
        amount: u32,
    },
}
