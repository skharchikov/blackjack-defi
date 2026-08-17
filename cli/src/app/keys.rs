use crossterm::event::KeyCode;
use tokio::sync::mpsc;

use crate::state::{GamePhase, LoginField, LoginStatus, Screen};

use super::event::AppEvent;
use super::state::App;

pub fn handle_key(app: &mut App, key: KeyCode, tx: &mpsc::Sender<AppEvent>) {
    if let KeyCode::Char('q') = key {
        // Don't quit on 'q' when typing in login fields
        if let Screen::Login(_) = &app.ui.screen {
            handle_login_key(app, key, tx);
            return;
        }
        app.should_quit = true;
        return;
    }

    match &app.ui.screen {
        Screen::Login(_) => handle_login_key(app, key, tx),
        Screen::Lobby(_) => handle_lobby_key(app, key, tx),
        Screen::Table(_) => handle_table_key(app, key, tx),
    }
}

fn handle_login_key(app: &mut App, key: KeyCode, tx: &mpsc::Sender<AppEvent>) {
    let Screen::Login(ref mut login) = app.ui.screen else {
        return;
    };

    match key {
        KeyCode::Tab | KeyCode::BackTab => {
            login.active_field = match login.active_field {
                LoginField::Username => LoginField::Password,
                LoginField::Password => LoginField::Username,
            };
        }
        KeyCode::Char(c) => match login.active_field {
            LoginField::Username => login.username.push(c),
            LoginField::Password => login.password.push(c),
        },
        KeyCode::Backspace => match login.active_field {
            LoginField::Username => {
                login.username.pop();
            }
            LoginField::Password => {
                login.password.pop();
            }
        },
        KeyCode::Enter if !login.username.is_empty() && !login.password.is_empty() => {
            app.username = login.username.clone();
            app.password = login.password.clone();
            login.status = LoginStatus::Connecting;
            crate::app::spawn_ws(app, tx);
        }
        KeyCode::Esc => {
            app.should_quit = true;
        }
        _ => {}
    }
}

fn handle_lobby_key(app: &mut App, key: KeyCode, _tx: &mpsc::Sender<AppEvent>) {
    let Screen::Lobby(ref mut lobby) = app.ui.screen else {
        return;
    };

    match key {
        KeyCode::Up if lobby.selected > 0 => {
            lobby.selected -= 1;
        }
        KeyCode::Down if lobby.selected + 1 < lobby.tables.len() => {
            lobby.selected += 1;
        }
        KeyCode::Enter => {
            if let Some(table) = lobby.tables.get(lobby.selected) {
                if table.is_joinable {
                    let table_id = table.id.clone();
                    app.table_min_bet = table.settings.min_bet;
                    app.table_max_bet = table.settings.max_bet;
                    app.current_table_id = Some(table_id.clone());
                    let rid = app.next_request_id();
                    if let Some(ref ws_tx) = app.ws_tx {
                        let join = serde_json::json!({"type": "JoinTable", "table_id": table_id, "request_id": rid});
                        let _ = ws_tx.try_send(join.to_string());
                    }
                }
            }
        }
        _ => {}
    }
}

fn handle_table_key(app: &mut App, key: KeyCode, tx: &mpsc::Sender<AppEvent>) {
    let Screen::Table(ref table) = app.ui.screen else {
        return;
    };
    let phase = table.phase;
    let is_observer = table.is_observer;

    // If outcome popup is visible, any key dismisses it
    if table.round_result.is_some() {
        if let Screen::Table(ref mut t) = app.ui.screen {
            t.round_result = None;
        }
        return;
    }

    if let KeyCode::Char('l') = key {
        let rid = app.next_request_id();
        if is_observer {
            // Observer leaves the table — keep the WS connection alive for the lobby.
            if let (Some(ref ws_tx), Some(ref tid)) = (&app.ws_tx, &app.current_table_id) {
                let msg =
                    serde_json::json!({"type": "LeaveTable", "table_id": tid, "request_id": rid});
                let _ = ws_tx.try_send(msg.to_string());
            }
            app.current_table_id = None;
            app.ui = crate::state::UiState::lobby();
        } else {
            // Seated/waiting player goes back to observer
            if let (Some(ref ws_tx), Some(ref tid)) = (&app.ws_tx, &app.current_table_id) {
                let msg =
                    serde_json::json!({"type": "LeaveSeat", "table_id": tid, "request_id": rid});
                let _ = ws_tx.try_send(msg.to_string());
            }
        }
        return;
    }

    // Observer: request a seat
    if is_observer {
        if let KeyCode::Char('t') = key {
            let rid = app.next_request_id();
            if let (Some(ref ws_tx), Some(ref tid)) = (&app.ws_tx, &app.current_table_id) {
                let msg =
                    serde_json::json!({"type": "TakeSeat", "table_id": tid, "request_id": rid});
                let _ = ws_tx.try_send(msg.to_string());
            }
        }
        let _ = tx;
        return;
    }

    if phase == GamePhase::Betting {
        handle_betting_key(app, key);
        return;
    }

    // PlayerTurn actions
    if phase == GamePhase::PlayerTurn {
        match key {
            KeyCode::Char('h') => {
                let rid = app.next_request_id();
                if let (Some(ref ws_tx), Some(ref tid)) = (&app.ws_tx, &app.current_table_id) {
                    let msg =
                        serde_json::json!({"type": "Hit", "table_id": tid, "request_id": rid});
                    let _ = ws_tx.try_send(msg.to_string());
                }
            }
            KeyCode::Char('s') => {
                let rid = app.next_request_id();
                if let (Some(ref ws_tx), Some(ref tid)) = (&app.ws_tx, &app.current_table_id) {
                    let msg =
                        serde_json::json!({"type": "Stand", "table_id": tid, "request_id": rid});
                    let _ = ws_tx.try_send(msg.to_string());
                }
            }
            KeyCode::Char('d') => {
                let rid = app.next_request_id();
                if let (Some(ref ws_tx), Some(ref tid)) = (&app.ws_tx, &app.current_table_id) {
                    let msg = serde_json::json!({"type": "DoubleDown", "table_id": tid, "request_id": rid});
                    let _ = ws_tx.try_send(msg.to_string());
                }
            }
            _ => {}
        }
    }

    // suppress unused warning: tx is threaded through for future use
    let _ = tx;
}

fn handle_betting_key(app: &mut App, key: KeyCode) {
    let Some(betting) = app.ui.betting.as_mut() else {
        return;
    };

    match key {
        KeyCode::Left => {
            betting.current_bet = betting
                .current_bet
                .saturating_sub(betting.step)
                .max(betting.min_bet);
        }
        KeyCode::Right => {
            betting.current_bet = (betting.current_bet + betting.step).min(betting.max_bet);
        }
        KeyCode::Enter => {
            let amount = betting.current_bet;
            betting.confirmed = true;
            let _ = betting;
            let rid = app.next_request_id();
            if let (Some(ref ws_tx), Some(ref tid)) = (&app.ws_tx, &app.current_table_id) {
                let msg = serde_json::json!({"type": "PlaceBet", "table_id": tid, "request_id": rid, "amount": amount});
                let _ = ws_tx.try_send(msg.to_string());
            }
        }
        _ => {}
    }
}
