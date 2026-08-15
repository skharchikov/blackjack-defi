use crate::domain::{
    dealer::DealerId,
    engine::{action::PlayerDecision, phase::Phase},
    player::PlayerId,
    Card, Seat,
};

use super::outcome::GameResult;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum EventPayload {
    PlayerJoined {
        player: PlayerId,
        seat: Seat,
    },
    PlayerLeft {
        player: PlayerId,
    },
    ObserverJoined {
        player: PlayerId,
    },
    ObserverLeft {
        player: PlayerId,
    },
    PlayerAddedToWaitingList {
        player: PlayerId,
        seat: Seat,
    },
    PlayerRemovedFromWaitingList {
        player: PlayerId,
    },
    PlayerPlacedBet {
        player: PlayerId,
        amount: u32,
    },
    /// Player doubled their stake; `amount` is the additional bet matching the original.
    PlayerDoubledDown {
        player: PlayerId,
        amount: u32,
    },
    GameStarted,
    PhaseChanged {
        from: Phase,
        to: Phase,
    },
    GameFinished {
        result: GameResult,
    },
    PlayerCardDealt {
        player: PlayerId,
        card: Card,
    },
    DealerCardDealt {
        dealer: DealerId,
        card: Card,
    },
    /// Hole card dealt face-down during initial dealing — card value not broadcast.
    DealerHoleCardDealt {
        dealer: DealerId,
    },
    /// Hole card revealed at the start of the dealer's turn.
    DealerHoleCardRevealed {
        dealer: DealerId,
        card: Card,
    },
    PlayerDecisionTaken {
        player: PlayerId,
        action: PlayerDecision,
    },
    PlayerBust {
        player: PlayerId,
    },
    DealerBust {
        dealer: DealerId,
    },
}
