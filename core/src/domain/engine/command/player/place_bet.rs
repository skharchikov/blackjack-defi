use crate::domain::{
    engine::{
        command::CommandHandler, error::CommandError, event::payload::EventPayload,
        game_state::GameState, phase::Phase,
    },
    player::PlayerId,
    table::TableSettings,
};

#[derive(Debug, Clone)]
pub struct PlaceBet {
    pub player_id: PlayerId,
    pub amount: u32,
}

impl CommandHandler for PlaceBet {
    fn handle(
        &self,
        state: &GameState,
        settings: &TableSettings,
    ) -> Result<Vec<EventPayload>, CommandError> {
        if !matches!(state.phase, Phase::WaitingForBets) {
            return Err(CommandError::WrongPhase {
                actual: state.phase.clone(),
            });
        }
        let player = state
            .player(self.player_id)
            .ok_or(CommandError::PlayerNotFound(self.player_id))?;
        if player.bet.is_some() {
            return Err(CommandError::AlreadyPlacedBet);
        }
        if self.amount < settings.min_bet {
            return Err(CommandError::BetBelowMinimum {
                min: settings.min_bet,
                amount: self.amount,
            });
        }
        if self.amount > settings.max_bet {
            return Err(CommandError::BetAboveMaximum {
                max: settings.max_bet,
                amount: self.amount,
            });
        }
        if self.amount > player.balance {
            return Err(CommandError::InsufficientBalance {
                balance: player.balance,
                amount: self.amount,
            });
        }
        Ok(vec![EventPayload::PlayerPlacedBet {
            player: self.player_id,
            amount: self.amount,
        }])
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
            GameEngine,
        },
        player::PlayerId,
        table::TableSettings,
        Shoe,
    };

    fn settings() -> TableSettings {
        TableSettings {
            min_bet: 10,
            max_bet: 500,
            max_players: 5,
            max_observers: 10,
        }
    }

    fn state_with_player(pid: PlayerId, balance: u32) -> GameState {
        GameState::new_with_balance(
            GameId::new(),
            Shoe::shuffled(),
            vec![(pid, balance)],
            DealerId::new(),
        )
    }

    fn bet_cmd(pid: PlayerId, amount: u32) -> GameCommand {
        GameCommand::Player(PlayerCommand {
            game_id: GameId::new(),
            command_id: CommandId(0),
            action: PlayerAction::PlaceBet(PlaceBet {
                player_id: pid,
                amount,
            }),
        })
    }

    #[test]
    fn place_bet_happy_path() {
        let pid = PlayerId::new();
        let state = state_with_player(pid, 1000);
        let events = GameEngine::handle(&state, &settings(), &bet_cmd(pid, 100)).unwrap();
        assert!(
            matches!(events[0], EventPayload::PlayerPlacedBet { player, amount: 100 } if player == pid)
        );
    }

    #[test]
    fn place_bet_wrong_phase() {
        let pid = PlayerId::new();
        let mut state = state_with_player(pid, 1000);
        state.phase = Phase::DealerTurn;
        assert!(matches!(
            GameEngine::handle(&state, &settings(), &bet_cmd(pid, 100)),
            Err(CommandError::WrongPhase { .. })
        ));
    }

    #[test]
    fn place_bet_below_minimum() {
        let pid = PlayerId::new();
        let state = state_with_player(pid, 1000);
        assert!(matches!(
            GameEngine::handle(&state, &settings(), &bet_cmd(pid, 5)),
            Err(CommandError::BetBelowMinimum { min: 10, amount: 5 })
        ));
    }

    #[test]
    fn place_bet_above_maximum() {
        let pid = PlayerId::new();
        let state = state_with_player(pid, 1000);
        assert!(matches!(
            GameEngine::handle(&state, &settings(), &bet_cmd(pid, 600)),
            Err(CommandError::BetAboveMaximum {
                max: 500,
                amount: 600
            })
        ));
    }

    #[test]
    fn place_bet_insufficient_balance() {
        let pid = PlayerId::new();
        let state = state_with_player(pid, 50);
        assert!(matches!(
            GameEngine::handle(&state, &settings(), &bet_cmd(pid, 100)),
            Err(CommandError::InsufficientBalance {
                balance: 50,
                amount: 100
            })
        ));
    }

    #[test]
    fn place_bet_already_placed() {
        let pid = PlayerId::new();
        let mut state = state_with_player(pid, 1000);
        let events = GameEngine::handle(&state, &settings(), &bet_cmd(pid, 100)).unwrap();
        for e in &events {
            state.apply_event(e);
        }
        assert!(matches!(
            GameEngine::handle(&state, &settings(), &bet_cmd(pid, 100)),
            Err(CommandError::AlreadyPlacedBet)
        ));
    }

    #[test]
    fn place_bet_player_not_found() {
        let state = GameState::new(GameId::new(), Shoe::shuffled(), vec![], DealerId::new());
        assert!(matches!(
            GameEngine::handle(&state, &settings(), &bet_cmd(PlayerId::new(), 100)),
            Err(CommandError::PlayerNotFound(_))
        ));
    }
}
