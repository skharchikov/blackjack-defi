use crate::domain::engine::phase::Phase;
use crate::domain::player::PlayerId;

#[derive(Debug, thiserror::Error, PartialEq, Clone)]
pub enum CommandError {
    #[error("command not valid in phase {actual:?}")]
    WrongPhase { actual: Phase },
    #[error("player {0:?} not found")]
    PlayerNotFound(PlayerId),
    #[error("not this player's turn")]
    NotPlayersTurn,
    #[error("bet {amount} is below minimum {min}")]
    BetBelowMinimum { min: u32, amount: u32 },
    #[error("bet {amount} exceeds maximum {max}")]
    BetAboveMaximum { max: u32, amount: u32 },
    #[error("insufficient balance: have {balance}, need {amount}")]
    InsufficientBalance { balance: u32, amount: u32 },
    #[error("player already placed a bet this round")]
    AlreadyPlacedBet,
    #[error("double down is only allowed on the initial two-card hand")]
    CannotDoubleDown,
    #[error("shoe is empty")]
    ShoeEmpty,
    #[error("table is full")]
    TableFull,
    #[error("observer capacity reached")]
    ObserversFull,
    #[error("no players have placed bets")]
    NoBettors,
    #[error("seat {0} is already occupied")]
    SeatOccupied(crate::domain::Seat),
    #[error("seat {0} is not available at this table (max {1} players)")]
    SeatNotAvailable(crate::domain::Seat, usize),
    #[error("no seats available at this table")]
    NoSeatAvailable,
}
