use crate::domain::{
    engine::{
        action::PlayerDecision, command::CommandHandler, error::CommandError,
        event::payload::EventPayload, game_state::GameState, phase::Phase,
    },
    player::PlayerId,
    table::TableSettings,
};

#[derive(Debug, Clone)]
pub struct DoubleDown {
    pub player_id: PlayerId,
}

impl CommandHandler for DoubleDown {
    fn handle(
        &self,
        state: &GameState,
        _settings: &TableSettings,
    ) -> Result<Vec<EventPayload>, CommandError> {
        match &state.phase {
            Phase::PlayerTurn(id) if *id == self.player_id => {}
            _ => return Err(CommandError::NotPlayersTurn),
        }
        let player = state
            .player(self.player_id)
            .ok_or(CommandError::PlayerNotFound(self.player_id))?;

        // Double down is only allowed on the initial two-card hand.
        if player.hand.cards.len() != 2 {
            return Err(CommandError::CannotDoubleDown);
        }

        let bet = player.bet.ok_or(CommandError::CannotDoubleDown)?;
        if player.balance < bet {
            return Err(CommandError::InsufficientBalance {
                balance: player.balance,
                amount: bet,
            });
        }

        let card = state.next_card().ok_or(CommandError::ShoeEmpty)?;

        let mut events = vec![
            EventPayload::PlayerDoubledDown {
                player: self.player_id,
                amount: bet,
            },
            EventPayload::PlayerCardDealt {
                player: self.player_id,
                card,
            },
            EventPayload::PlayerDecisionTaken {
                player: self.player_id,
                action: PlayerDecision::Double,
            },
        ];

        let mut new_hand = player.hand.clone();
        new_hand.add_card(card);
        if new_hand.value().is_bust() {
            events.push(EventPayload::PlayerBust {
                player: self.player_id,
            });
        }

        events.push(EventPayload::PhaseChanged {
            from: Phase::PlayerTurn(self.player_id),
            to: state.next_player_after(self.player_id),
        });

        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        dealer::DealerId,
        engine::{
            action::PlayerDecision,
            command::{
                player::{PlayerAction, PlayerCommand},
                CommandId, GameCommand,
            },
            game_id::GameId,
            game_state::GameState,
            phase::Phase,
            GameEngine,
        },
        player::PlayerId,
        table::TableSettings,
        Card, DeckId, Rank, Suit,
    };

    fn settings() -> TableSettings {
        TableSettings {
            min_bet: 10,
            max_bet: 500,
            max_players: 5,
            max_observers: 10,
        }
    }

    fn card(rank: Rank) -> Card {
        Card {
            deck_id: DeckId::One,
            rank,
            suit: Suit::Spades,
        }
    }

    fn double_cmd(pid: PlayerId) -> GameCommand {
        GameCommand::Player(PlayerCommand {
            game_id: GameId::new(),
            command_id: CommandId(0),
            action: PlayerAction::DoubleDown(DoubleDown { player_id: pid }),
        })
    }

    fn state_in_player_turn(
        pid: PlayerId,
        hand_ranks: Vec<Rank>,
        next_card_rank: Rank,
        bet: u32,
        balance: u32,
    ) -> GameState {
        let mut shoe: Vec<Card> = vec![card(next_card_rank)];
        shoe.extend(vec![card(Rank::Two); 20]);

        let mut state =
            GameState::new_with_balance(GameId::new(), shoe, vec![(pid, balance)], DealerId::new());
        state.players[0].bet = Some(bet);
        for r in hand_ranks {
            state.players[0].hand.add_card(card(r));
        }
        state.phase = Phase::PlayerTurn(pid);
        state
    }

    #[test]
    fn double_deals_one_card_and_stands() {
        let pid = PlayerId::new();
        // Two + Three = 5, next Four -> 9, no bust
        let state = state_in_player_turn(pid, vec![Rank::Two, Rank::Three], Rank::Four, 100, 1000);
        let events = GameEngine::handle(&state, &settings(), &double_cmd(pid)).unwrap();
        // DoubledDown + CardDealt + DecisionTaken(Double) + PhaseChanged
        assert_eq!(events.len(), 4);
        assert!(matches!(
            events[0],
            EventPayload::PlayerDoubledDown { amount: 100, .. }
        ));
        assert!(matches!(events[1], EventPayload::PlayerCardDealt { .. }));
        assert!(matches!(
            events[2],
            EventPayload::PlayerDecisionTaken {
                action: PlayerDecision::Double,
                ..
            }
        ));
        assert!(matches!(events[3], EventPayload::PhaseChanged { .. }));
    }

    #[test]
    fn double_doubles_bet_and_deducts_balance() {
        let pid = PlayerId::new();
        let mut state =
            state_in_player_turn(pid, vec![Rank::Two, Rank::Three], Rank::Four, 100, 1000);
        let events = GameEngine::handle(&state, &settings(), &double_cmd(pid)).unwrap();
        for e in &events {
            state.apply_event(e);
        }
        assert_eq!(state.players[0].bet, Some(200));
        assert_eq!(state.players[0].balance, 900);
    }

    #[test]
    fn double_causes_bust() {
        let pid = PlayerId::new();
        // King(10) + Queen(10) = 20, next Five(5) -> 25 bust
        let state = state_in_player_turn(pid, vec![Rank::King, Rank::Queen], Rank::Five, 100, 1000);
        let events = GameEngine::handle(&state, &settings(), &double_cmd(pid)).unwrap();
        // DoubledDown + CardDealt + DecisionTaken + Bust + PhaseChanged
        assert_eq!(events.len(), 5);
        assert!(matches!(events[3], EventPayload::PlayerBust { .. }));
        assert!(matches!(events[4], EventPayload::PhaseChanged { .. }));
    }

    #[test]
    fn double_wrong_turn() {
        let pid = PlayerId::new();
        let other = PlayerId::new();
        let state = state_in_player_turn(pid, vec![Rank::Two, Rank::Three], Rank::Four, 100, 1000);
        assert!(matches!(
            GameEngine::handle(&state, &settings(), &double_cmd(other)),
            Err(CommandError::NotPlayersTurn)
        ));
    }

    #[test]
    fn double_not_two_cards() {
        let pid = PlayerId::new();
        // three cards in hand -> cannot double
        let state = state_in_player_turn(
            pid,
            vec![Rank::Two, Rank::Three, Rank::Four],
            Rank::Five,
            100,
            1000,
        );
        assert!(matches!(
            GameEngine::handle(&state, &settings(), &double_cmd(pid)),
            Err(CommandError::CannotDoubleDown)
        ));
    }

    #[test]
    fn double_then_win_pays_out_on_doubled_stake() {
        use crate::domain::engine::command::dealer::{DealerAction, DealerCommand, SettleRound};

        let pid = PlayerId::new();
        // Player: Six + Five = 11, doubles, draws Nine -> 20.
        let mut state =
            state_in_player_turn(pid, vec![Rank::Six, Rank::Five], Rank::Nine, 100, 1000);
        // Balance after the original bet was placed: place_bet already deducted it.
        state.players[0].balance = 900;

        let events = GameEngine::handle(&state, &settings(), &double_cmd(pid)).unwrap();
        for e in &events {
            state.apply_event(e);
        }
        // Stake doubled to 200, balance now 800.
        assert_eq!(state.players[0].bet, Some(200));
        assert_eq!(state.players[0].balance, 800);

        // Dealer stands on 18; player 20 wins.
        state.dealer.hand.add_card(card(Rank::King));
        state.dealer.hand.add_card(card(Rank::Eight));
        state.phase = Phase::Payouts;

        let settle = GameCommand::Dealer(DealerCommand {
            game_id: GameId::new(),
            command_id: CommandId(0),
            action: DealerAction::SettleRound(SettleRound),
        });
        let settle_events = GameEngine::handle(&state, &settings(), &settle).unwrap();
        for e in &settle_events {
            state.apply_event(e);
        }
        // Win pays 2x the 200 stake = 400 credited; 800 + 400 = 1200.
        assert_eq!(state.players[0].balance, 1200);
    }

    #[test]
    fn double_advances_to_next_player() {
        let p1 = PlayerId::new();
        let p2 = PlayerId::new();
        let mut shoe: Vec<Card> = vec![card(Rank::Four)];
        shoe.extend(vec![card(Rank::Two); 20]);
        let mut state = GameState::new_with_balance(
            GameId::new(),
            shoe,
            vec![(p1, 1000), (p2, 1000)],
            DealerId::new(),
        );
        state.players[0].bet = Some(100);
        state.players[0].hand.add_card(card(Rank::Two));
        state.players[0].hand.add_card(card(Rank::Three));
        state.players[1].bet = Some(100);
        state.phase = Phase::PlayerTurn(p1);

        let events = GameEngine::handle(&state, &settings(), &double_cmd(p1)).unwrap();
        let last = events.last().unwrap();
        assert!(
            matches!(last, EventPayload::PhaseChanged { to: Phase::PlayerTurn(id), .. } if *id == p2)
        );
    }

    #[test]
    fn double_with_no_bet_placed() {
        let pid = PlayerId::new();
        // In PlayerTurn with a two-card hand but no bet -> cannot double.
        let mut state =
            state_in_player_turn(pid, vec![Rank::Two, Rank::Three], Rank::Four, 100, 1000);
        state.players[0].bet = None;
        assert!(matches!(
            GameEngine::handle(&state, &settings(), &double_cmd(pid)),
            Err(CommandError::CannotDoubleDown)
        ));
    }

    #[test]
    fn double_insufficient_balance() {
        let pid = PlayerId::new();
        // bet 100 but balance only 50 -> cannot cover the doubled stake
        let state = state_in_player_turn(pid, vec![Rank::Two, Rank::Three], Rank::Four, 100, 50);
        assert!(matches!(
            GameEngine::handle(&state, &settings(), &double_cmd(pid)),
            Err(CommandError::InsufficientBalance {
                balance: 50,
                amount: 100
            })
        ));
    }
}
