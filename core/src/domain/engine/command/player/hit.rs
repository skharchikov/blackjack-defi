use crate::domain::{
    engine::{
        action::PlayerDecision, command::CommandHandler, error::CommandError,
        event::payload::EventPayload, game_state::GameState, phase::Phase,
    },
    player::PlayerId,
    table::TableSettings,
};

#[derive(Debug, Clone)]
pub struct Hit {
    pub player_id: PlayerId,
}

impl CommandHandler for Hit {
    fn handle(
        &self,
        state: &GameState,
        _settings: &TableSettings,
    ) -> Result<Vec<EventPayload>, CommandError> {
        match &state.phase {
            Phase::PlayerTurn(id) if *id == self.player_id => {}
            _ => return Err(CommandError::NotPlayersTurn),
        }
        let card = state.next_card().ok_or(CommandError::ShoeEmpty)?;
        let player = state
            .player(self.player_id)
            .ok_or(CommandError::PlayerNotFound(self.player_id))?;

        let mut events = vec![EventPayload::PlayerCardDealt {
            player: self.player_id,
            card,
        }];

        let mut new_hand = player.hand.clone();
        new_hand.add_card(card);

        if new_hand.value().is_bust() {
            events.push(EventPayload::PlayerBust {
                player: self.player_id,
            });
            events.push(EventPayload::PhaseChanged {
                from: Phase::PlayerTurn(self.player_id),
                to: state.next_player_after(self.player_id),
            });
        } else if new_hand.value().best_value() == 21 {
            // Auto-stand on 21
            events.push(EventPayload::PlayerDecisionTaken {
                player: self.player_id,
                action: PlayerDecision::Stand,
            });
            events.push(EventPayload::PhaseChanged {
                from: Phase::PlayerTurn(self.player_id),
                to: state.next_player_after(self.player_id),
            });
        }

        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        dealer::DealerId,
        engine::{
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

    fn hit_cmd(pid: PlayerId) -> GameCommand {
        GameCommand::Player(PlayerCommand {
            game_id: GameId::new(),
            command_id: CommandId(0),
            action: PlayerAction::Hit(Hit { player_id: pid }),
        })
    }

    fn state_in_player_turn(
        pid: PlayerId,
        hand_ranks: Vec<Rank>,
        next_card_rank: Rank,
    ) -> GameState {
        // shoe: [next_card, padding...]
        let mut shoe: Vec<Card> = vec![card(next_card_rank)];
        shoe.extend(vec![card(Rank::Two); 20]);

        let mut state =
            GameState::new_with_balance(GameId::new(), shoe, vec![(pid, 1000)], DealerId::new());
        state.players[0].bet = Some(100);
        // Manually add hand cards (dealt = 0, so next_card is shoe[0])
        for r in hand_ranks {
            state.players[0].hand.add_card(card(r));
        }
        state.phase = Phase::PlayerTurn(pid);
        state
    }

    #[test]
    fn hit_deals_card_no_bust() {
        let pid = PlayerId::new();
        let state = state_in_player_turn(pid, vec![Rank::Two, Rank::Three], Rank::Four);
        let events = GameEngine::handle(&state, &settings(), &hit_cmd(pid)).unwrap();
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], EventPayload::PlayerCardDealt { .. }));
    }

    #[test]
    fn hit_causes_bust() {
        let pid = PlayerId::new();
        // King(10) + Queen(10) in hand, next card Five(5) -> 25, bust
        let state = state_in_player_turn(pid, vec![Rank::King, Rank::Queen], Rank::Five);
        let events = GameEngine::handle(&state, &settings(), &hit_cmd(pid)).unwrap();
        assert_eq!(events.len(), 3); // CardDealt + Bust + PhaseChanged
        assert!(matches!(events[1], EventPayload::PlayerBust { .. }));
        assert!(matches!(events[2], EventPayload::PhaseChanged { .. }));
    }

    #[test]
    fn hit_reaches_21_auto_stand() {
        let pid = PlayerId::new();
        // King(10) + Eight(8) in hand, next Three(3) -> 21
        let state = state_in_player_turn(pid, vec![Rank::King, Rank::Eight], Rank::Three);
        let events = GameEngine::handle(&state, &settings(), &hit_cmd(pid)).unwrap();
        assert_eq!(events.len(), 3); // CardDealt + DecisionTaken(Stand) + PhaseChanged
        assert!(matches!(
            events[1],
            EventPayload::PlayerDecisionTaken {
                action: PlayerDecision::Stand,
                ..
            }
        ));
    }

    #[test]
    fn hit_wrong_turn() {
        let pid = PlayerId::new();
        let other = PlayerId::new();
        let state = state_in_player_turn(pid, vec![Rank::Two, Rank::Three], Rank::Four);
        assert!(matches!(
            GameEngine::handle(&state, &settings(), &hit_cmd(other)),
            Err(CommandError::NotPlayersTurn)
        ));
    }
}
