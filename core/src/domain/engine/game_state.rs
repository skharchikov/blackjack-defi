use crate::domain::{
    dealer::{DealerId, DealerState},
    engine::{game_id::GameId, phase::Phase},
    player::{PlayerId, PlayerState},
    Card, Seat,
};

use super::event::EventPayload;

#[derive(Debug, Clone)]
pub struct GameState {
    pub game_id: GameId,
    pub phase: Phase,
    pub shoe: Vec<Card>,
    pub dealt: usize,
    pub players: Vec<PlayerState>,
    pub dealer: DealerState,
    pub observers: Vec<PlayerId>,
    /// (player_id, desired_seat) — seat is remembered so OpenBetting can restore the player to their preferred position.
    pub waiting: Vec<(PlayerId, Seat)>,
}

impl GameState {
    pub fn new(game_id: GameId, shoe: Vec<Card>, players: Vec<PlayerId>, dealer: DealerId) -> Self {
        debug_assert!(
            players.len() <= Seat::ALL.len(),
            "more players than seats: {}",
            players.len()
        );
        Self {
            game_id,
            phase: Phase::WaitingForBets,
            shoe,
            dealt: 0,
            players: players
                .into_iter()
                .enumerate()
                .map(|(i, id)| PlayerState::at_seat(id, Seat::ALL[i]))
                .collect(),
            dealer: DealerState::new(dealer),
            observers: vec![],
            waiting: vec![],
        }
    }

    /// Creates a `GameState` with players that have a specific starting balance.
    pub fn new_with_balance(
        game_id: GameId,
        shoe: Vec<Card>,
        players: Vec<(PlayerId, u32)>,
        dealer: DealerId,
    ) -> Self {
        debug_assert!(
            players.len() <= Seat::ALL.len(),
            "more players than seats: {}",
            players.len()
        );
        Self {
            game_id,
            phase: Phase::WaitingForBets,
            shoe,
            dealt: 0,
            players: players
                .into_iter()
                .enumerate()
                .map(|(i, (id, balance))| {
                    PlayerState::with_balance_at_seat(id, Seat::ALL[i], balance)
                })
                .collect(),
            dealer: DealerState::new(dealer),
            observers: vec![],
            waiting: vec![],
        }
    }

    /// Returns the player with the given id, if seated at this table.
    pub fn player(&self, id: PlayerId) -> Option<&PlayerState> {
        self.players.iter().find(|p| p.player_id == id)
    }

    /// Returns a mutable reference to the player with the given id, if seated at this table.
    pub fn player_mut(&mut self, id: PlayerId) -> Option<&mut PlayerState> {
        self.players.iter_mut().find(|p| p.player_id == id)
    }

    /// Returns the next card to be dealt from the shoe, without advancing the cursor.
    pub fn next_card(&self) -> Option<Card> {
        self.shoe.get(self.dealt).copied()
    }

    pub fn cards_remaining(&self) -> usize {
        self.shoe.len().saturating_sub(self.dealt)
    }

    pub fn apply_event(&mut self, payload: &EventPayload) {
        match payload {
            EventPayload::PlayerJoined { player, seat } => {
                self.observers.retain(|&p| p != *player);
                self.waiting.retain(|(p, _)| *p != *player);
                self.players.push(PlayerState::at_seat(*player, *seat));
            }
            EventPayload::PlayerLeft { player } => {
                self.players.retain(|p| p.player_id != *player);
                self.waiting.retain(|(p, _)| *p != *player);
            }
            EventPayload::ObserverJoined { player } => {
                self.observers.push(*player);
            }
            EventPayload::ObserverLeft { player } => {
                self.observers.retain(|&p| p != *player);
            }
            EventPayload::PlayerAddedToWaitingList { player, seat } => {
                self.observers.retain(|&p| p != *player);
                self.waiting.push((*player, *seat));
            }
            EventPayload::PlayerRemovedFromWaitingList { player } => {
                self.waiting.retain(|(p, _)| *p != *player);
            }
            EventPayload::PlayerPlacedBet { player, amount } => {
                if let Some(player_state) = self.player_mut(*player) {
                    player_state.balance = player_state.balance.saturating_sub(*amount);
                    player_state.bet = Some(*amount);
                }
            }
            EventPayload::PlayerDoubledDown { player, amount } => {
                if let Some(player_state) = self.player_mut(*player) {
                    player_state.balance = player_state.balance.saturating_sub(*amount);
                    player_state.bet = player_state.bet.map(|b| b + *amount);
                }
            }
            EventPayload::GameStarted => {
                self.phase = Phase::InitialDealing;
            }
            EventPayload::PhaseChanged { from: _, to } => {
                self.phase = to.clone();
            }
            EventPayload::GameFinished { result } => {
                self.phase = Phase::Finished;
                for player_result in &result.player_results {
                    if let Some(player_state) = self.player_mut(player_result.player) {
                        player_state.balance += player_result.payout.total();
                    }
                }
            }
            EventPayload::PlayerCardDealt { player, card } => {
                // Increment the deal cursor only when the card was actually dealt to a
                // seated player, matching the original inline lookup's behaviour.
                if self
                    .player_mut(*player)
                    .map(|p| p.hand.add_card(*card))
                    .is_some()
                {
                    self.dealt += 1;
                }
            }
            EventPayload::DealerCardDealt { dealer: _, card } => {
                self.dealer.hand.add_card(*card);
                self.dealt += 1;
            }
            EventPayload::DealerHoleCardDealt { dealer: _ } => {
                let card = *self
                    .shoe
                    .get(self.dealt)
                    .expect("shoe exhausted on hole card deal — dealing logic bug");
                self.dealer.hand.add_card(card);
                self.dealt += 1;
            }
            EventPayload::DealerHoleCardRevealed { dealer: _, card: _ } => {
                // State already has the card; this event exists only to inform clients.
            }
            EventPayload::PlayerDecisionTaken { player, action } => {
                if let Some(player_state) = self.player_mut(*player) {
                    player_state.decisions.push(*action);
                }
            }
            EventPayload::PlayerBust { player: _ } => {
                // Player has busted, hand value already reflects this
            }
            EventPayload::DealerBust { dealer: _ } => {
                // Dealer has busted, hand value already reflects this
            }
        }
    }

    pub fn next_player_after(&self, current_id: PlayerId) -> Phase {
        let idx = match self.players.iter().position(|p| p.player_id == current_id) {
            Some(i) => i,
            None => return Phase::DealerTurn,
        };
        for p in &self.players[idx + 1..] {
            if p.bet.is_some() && !self.player_finished(p) {
                return Phase::PlayerTurn(p.player_id);
            }
        }
        Phase::DealerTurn
    }

    pub fn next_player_after_leave(&self) -> Phase {
        for p in &self.players {
            if p.bet.is_some() && !self.player_finished(p) {
                return Phase::PlayerTurn(p.player_id);
            }
        }
        Phase::DealerTurn
    }

    pub fn player_finished(&self, p: &PlayerState) -> bool {
        use crate::domain::engine::action::PlayerDecision;
        p.hand.value().is_bust()
            || matches!(
                p.decisions.last(),
                Some(&PlayerDecision::Stand) | Some(&PlayerDecision::Double)
            )
    }

    pub fn first_betting_player(&self) -> Option<PlayerId> {
        self.players
            .iter()
            .find(|p| p.bet.is_some())
            .map(|p| p.player_id)
    }
}
