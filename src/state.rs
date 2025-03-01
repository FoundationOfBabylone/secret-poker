use secret_toolkit_serialization::Json;
use secret_toolkit_storage::{Item, Keymap, KeymapBuilder, WithoutIter};
use serde::{Deserialize, Serialize};
use cosmwasm_std::{Addr, StdError, StdResult, Storage, Timestamp};
use uuid::Uuid;

pub const PREFIX_REVOKED_PERMITS: &str = "revoked_permits";

pub static COUNTER_KEY: Item<u128> = Item::new(b"counter");

pub static CONFIG_KEY: Item<Config> = Item::new(b"config");

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub owner: Addr,
    pub contract_address: Addr,
}

pub static TABLES_STORE: Keymap<u32, PokerTable, Json, WithoutIter> =
            KeymapBuilder::new(b"tables").without_iter().build();

pub fn save_table(storage: &mut dyn Storage, key: u32, item: &PokerTable) -> StdResult<()> {
    TABLES_STORE.insert(storage, &key, item).map_err(|err| {
        StdError::generic_err(format!("Failed to save table: {}", err))
    })
}

pub fn load_table(storage: &dyn Storage, key: u32) -> Option<PokerTable> {
    TABLES_STORE.get(storage, &key)
}

pub fn delete_table(storage: &mut dyn Storage, key: u32) -> StdResult<()> {
    TABLES_STORE.remove(storage, &key).map_err(|err| {
        StdError::generic_err(format!("Failed to delete table: {}", err))
    })
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct CommunityCards {
    pub flop: Flop,
    pub turn: Turn, 
    pub river: River, 
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Flop {
    pub cards: Vec<Card>,
    pub secret: u64,
    pub retrieved_at: Option<Timestamp>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Turn {
    pub card: Card,
    pub secret: u64,
    pub retrieved_at: Option<Timestamp>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct River {
    pub card: Card,
    pub secret: u64,
    pub retrieved_at: Option<Timestamp>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PokerTable {
    pub hand_ref: u32,
    pub players: Vec<Player>,
    pub community_cards: CommunityCards,
    pub showdown_retrieved_at: Option<Timestamp>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Player {
    pub username: String,
    pub player_id: Uuid,
    pub public_key: String,
    pub hand: Vec<Card>,
    pub hand_secret: u64,
    pub flop_secret_share: u64,
    pub turn_secret_share: u64,
    pub river_secret_share: u64,
}



#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GameState {
    PreFlop,
    Flop,
    Turn,
    River,
}


#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Card(u8);

impl Card {
    pub fn new(suit: u8, rank: u8) -> Self {
        assert!(suit < 4, "Invalid suit");
        assert!(rank >= 1 && rank <= 13, "Invalid rank");
        Card((suit << 4) | rank)
    }

    pub fn suit(&self) -> u8 {
        self.0 >> 4
    }

    pub fn rank(&self) -> u8 {
        self.0 & 0b1111
    }

    pub fn to_bytes(&self) -> u8 {
        self.0
    }

    pub fn from_bytes(byte: u8) -> Self {
        Card(byte)
    }

    pub fn to_string(&self) -> String {
        /* Order of suits in this list is relatively important (as they are mostly continuous digits, ranks is pretty hard to f*** up...),
         * this list of suits should be in the same order in the backend and frontend executing/querying the contract.
         * This order is crucial because the contract logs the cards from the last game 
         * (for audit purposes) in the transaction log (unencrypted plaintext) of each StartGameResponse. 
         * Thus, by doing so, the last_hand_log will match what the player sees in his game, and what will be stored in the
         * backend database. Anyways, for audit purposes it's not a big deal, we can always map the suits to the correct ones by permutation.
         */ 
        let suits = ["♣", "♦", "♥", "♠"]; 
        let ranks = ["A", "2", "3", "4", "5", "6", "7", "8", "9", "10", "J", "Q", "K"];
        format!("{}{}", suits[self.suit() as usize], ranks[self.rank() as usize - 1])
    }
}


#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Deck {
    pub cards: Vec<Card>,
}

impl Deck {
    pub fn new() -> Self {
        let mut cards = Vec::new();
        for suit in 0..4 {
            for rank in 1..=13 {
                cards.push(Card::new(suit, rank));
            }
        }
        Deck { cards }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        self.cards.iter().map(|card| card.0).collect()
    }

    pub fn from_bytes(bytes: &[u8]) -> Self {
        let cards = bytes.iter().map(|&b| Card(b)).collect();
        Deck { cards }
    }
}


#[cfg(test)]
mod tests {

use super::*;
    #[test]
    fn cards() {
        let deck = Deck::new();
        for card in deck.cards.iter() {
            println!("{}", card.to_bytes());
            println!("{}", card.to_string());
        }
    }
}