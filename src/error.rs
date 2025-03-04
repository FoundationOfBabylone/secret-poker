use cosmwasm_std::StdError;
use thiserror::Error;

use crate::state::GameState;

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    // let thiserror implement From<StdError> for you
    Std(#[from] StdError),

    #[error("Unauthorized")]
    // issued when message sender != owner
    Unauthorized {},

    #[error("Game state error in method {method} for table {table_id}: got {game_state:?}")]
    // issued when game state is invalid
    GameStateError {
        method: String,
        table_id: u32,
        game_state: Option<GameState>,
    },

    #[error("Cards already retrieved by contract owner...")]
    //This should never arise, that would mean the contract owner tried to retrieve the cards twice
    CardsAlreadyRetrieved {},

    #[error("Player {player} not found in table {table_id}")]
    // issued when player is not found
    PlayerNotFound { table_id: u32, player: String },

    #[error("Table {table_id} not found")]
    // issued when table is not found
    TableNotFound { table_id: u32 },

    #[error("Custom Error val: {val}")]
    CustomError { val: String },
    // Add any other custom errors you like here.
    // Look at https://docs.rs/thiserror/1.0.21/thiserror/ for details.

    #[error("Serialization error: {error}")]
    // issued when serialization fails
    SerializationFailed {error: String},

    #[error("Duplicate public key")]
    // issued when public key is already in use
    DuplicatePublicKeys {},

    #[error("Players invalide count: {count}")]
    // issued when player count is invalid
    InvalidPlayerCount { count: usize },
}