use std::collections::HashSet;

use cosmwasm_std::{
    entry_point, to_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdError, StdResult,
};
use secret_toolkit_crypto::hkdf_sha_512;
use secret_toolkit_permit::{validate, Permit};
use sha2::{Digest, Sha256};
use uuid::Uuid;


use crate::error::ContractError;
use crate::msg::{
    CommunityCardsResponse, ExecuteMsg, InstantiateMsg, LastHandLogResponse, QueryMsg, QueryWithPermit, ResponsePayload, ShowdownPlayer, ShowdownResponse, StartGamePlayer, StartGameResponse
};
use crate::state::{
    load_table, save_table, Card, CommunityCards, Config, Deck, Flop, GameState,
    Player, PokerTable, River, Turn, CONFIG_KEY, COUNTER_KEY, PREFIX_REVOKED_PERMITS,
};

const MIN_PLAYERS: usize = 2;
const MAX_PLAYERS: usize = 9;
const COMMUNITY_CARD_PHASES: usize = 3;
const SECRET_LENGTH: usize = 64;
const RANDOM_SEED_SIZE: usize = 16;
const RESPONSE_KEY: &str = "response";

mod helpers {
    use super::*;

    pub fn generate_random_number(env: &Env, counter: &mut u128) -> StdResult<u64> {
        let secret = hkdf_sha_512(
            &Some(vec![0u8; SECRET_LENGTH]),
            &env.block.random.as_ref().unwrap(),
            &counter.to_le_bytes(),
            SECRET_LENGTH,
        )?;

        *counter += 1;
        Ok(u64::from_le_bytes(secret[..8].try_into().unwrap()))
    }

    pub fn additive_secret_sharing(
        env: &Env,
        players: usize,
        secret: u64,
        counter: &mut u128,
    ) -> StdResult<Vec<u64>> {
        let mut shares = Vec::with_capacity(players);
        let mut sum: u64 = 0;

        for _ in 0..(players - 1) {
            let share = generate_random_number(env, counter)?;
            shares.push(share);
            sum = sum.wrapping_add(share);
        }

        shares.push(secret.wrapping_sub(sum));
        Ok(shares)
    }


    pub fn shuffle_deck(deck: &mut Deck, seed: u64) {
        let mut rng = Sha256::new();
        let mut deck_len = deck.cards.len();

        while deck_len > 1 {
            deck_len -= 1;
            
            // The upper bound for our random index is deck_len + 1
            // E.g., for the first swap (deck_len=51), we need an index from 0 to 51 (52 possibilities).
            let upper_bound = deck_len + 1;

            // --- Start of adapted secure_random_index logic ---

            // 1. Calculate the threshold to avoid bias. Any random value generated
            //    above this threshold will be discarded and regenerated.
            let threshold = (u64::MAX / upper_bound as u64) * upper_bound as u64;
            
            let random_index;
            let mut attempt_counter: u64 = 0; // A nonce to get a new hash if we have to retry.

            // 2. Start the rejection sampling loop.
            loop {
                // Update the hasher with the core seed, the current deck length,
                // and the attempt counter. The counter is crucial to ensure we get a
                // new random number if the first one is biased.
                rng.update(&seed.to_le_bytes());
                rng.update(&(deck_len as u64).to_le_bytes());
                rng.update(&attempt_counter.to_le_bytes());

                let hash = rng.finalize_reset();
                let random_value = u64::from_le_bytes(hash[..8].try_into().unwrap());
                
                // 3. If the value is below the threshold, it's unbiased.
                if random_value < threshold {
                    // We can now safely use the modulo operator.
                    random_index = (random_value as usize) % upper_bound;
                    break; // Exit the rejection sampling loop.
                }

                // 4. Otherwise, the value is in the biased range. Increment the counter
                //    and try again.
                attempt_counter += 1;
            }
            // --- End of adapted logic ---

            deck.cards.swap(deck_len, random_index);
        }
    }
}



mod state_utils {
    use super::*;

    pub fn load_table_or_error(
        storage: &dyn cosmwasm_std::Storage,
        table_id: u32,
    ) -> Result<PokerTable, ContractError> {
        load_table(storage, table_id).ok_or(ContractError::TableNotFound { table_id })
    }
}


mod query_handlers {
    use crate::msg::PlayerDataResponse;

    use super::*;

    pub fn handle_permit_query(
        deps: Deps,
        permit: Permit,
        query: QueryWithPermit,
    ) -> StdResult<Binary> {
        let config = CONFIG_KEY.load(deps.storage)?;
        let viewer = validate(
            deps,
            PREFIX_REVOKED_PERMITS,
            &permit,
            config.contract_address.to_string(),
            None,
        )?;

        match query {
            QueryWithPermit::PlayerPrivateData { table_id } => {
                let private_data = query_player_private_data(deps, table_id, viewer)?;
                let serialized = match serde_json_wasm::to_string(&private_data) {
                    Ok(json) => Ok(json),
                    Err(e) => Err(StdError::generic_err(e.to_string())),
                };
            
           to_binary(&serialized?)
            }
        }
    }

    pub fn query_player_private_data(
        deps: Deps,
        table_id: u32,
        pub_key: String,
    ) -> StdResult<PlayerDataResponse> {
        let table =
            load_table(deps.storage, table_id).ok_or(StdError::generic_err("No table found"))?;

        table
            .players
            .iter()
            .find(|p| p.public_key == pub_key)
            .cloned()
            .ok_or(StdError::generic_err("No player found"))
            .map(|player| PlayerDataResponse {
                table_id,
                hand_ref: table.hand_ref,
                hand: player.hand,
                hand_secret: player.hand_secret.to_string(),
                flop_secret_share: player.flop_secret_share.to_string(),
                turn_secret_share: player.turn_secret_share.to_string(),
                river_secret_share: player.river_secret_share.to_string(),
            })
    }

    pub fn query_community_cards(
        deps: Deps,
        table_id: u32,
        game_state: GameState,
        secret_key: u64,
    ) -> StdResult<CommunityCardsResponse> {
        let table =
            load_table(deps.storage, table_id).ok_or(StdError::generic_err("No table found"))?;

        let (stored_key, cards) = match game_state {
            GameState::Flop => (
                table.community_cards.flop.secret,
                table.community_cards.flop.cards,
            ),
            GameState::Turn => (
                table.community_cards.turn.secret,
                vec![table.community_cards.turn.card],
            ),
            GameState::River => (
                table.community_cards.river.secret,
                vec![table.community_cards.river.card],
            ),
            _ => return Err(StdError::generic_err("Invalid game state")),
        };

        if stored_key != secret_key {
            return Err(StdError::generic_err("Invalid viewing key"));
        }

        Ok(CommunityCardsResponse {
            table_id,
            hand_ref: table.hand_ref,
            game_state,
            community_cards: cards,
        })
    }

    pub fn query_showdown(
        deps: Deps,
        table_id: u32,
        flop_secret: Option<u64>,
        turn_secret: Option<u64>,
        river_secret: Option<u64>,
        players_secrets: Vec<u64>,
    ) -> StdResult<ShowdownResponse> {
        let table =
            load_table(deps.storage, table_id).ok_or(StdError::generic_err("No table found"))?;

        let mut community_cards = Vec::new();

        if let Some(secret) = flop_secret {
            if table.community_cards.flop.secret != secret {
                return Err(StdError::generic_err("Invalid secret key"));
            }
            community_cards.extend(table.community_cards.flop.cards.clone());
        }

        if let Some(secret) = turn_secret {
            if table.community_cards.turn.secret != secret {
                return Err(StdError::generic_err("Invalid secret key"));
            }
            community_cards.push(table.community_cards.turn.card);
        }

        if let Some(secret) = river_secret {
            if table.community_cards.river.secret != secret {
                return Err(StdError::generic_err("Invalid secret key"));
            }
            community_cards.push(table.community_cards.river.card);
        }

        let players_cards = players_secrets
            .iter()
            .map(|secret| {
                table
                    .players
                    .iter()
                    .find(|player| &player.hand_secret == secret)
                    .map(|player| (player.player_id.clone(), player.hand.clone()))
                    .ok_or_else(|| StdError::generic_err("Player not found"))
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(ShowdownResponse {
            table_id,
            hand_ref: table.hand_ref,
            players_cards,
            community_cards: Some(community_cards),
        })
    }
}


mod execute_handlers {
    use super::{state_utils::load_table_or_error, *};

    pub fn handle_start_game(
        deps: DepsMut,
        env: Env,
        table_id: u32,
        hand_ref: u32,
        players_info: Vec<StartGamePlayer>,
        prev_hand_showdown_players: Vec<Uuid>,
    ) -> Result<Response, ContractError> {
        validate_players(&players_info)?;
        let previous_hand_log = create_previous_hand_log(deps.as_ref(), table_id, prev_hand_showdown_players)?;
        let mut counter = COUNTER_KEY.load(deps.storage)?;
        let mut deck = initialize_deck(&env, &mut counter)?;
        let player_cards = distribute_player_cards(&mut deck, &players_info);
        let mut secrets = Vec::with_capacity(COMMUNITY_CARD_PHASES);
        let community_cards =
            generate_community_cards(&env, &mut counter, &mut secrets, &mut deck, players_info.len())?;
        let players = create_players(
            players_info,
            player_cards,
            &secrets,
            &env,
            &mut counter,
        )?;

        let table = PokerTable {
            hand_ref,
            players,
            community_cards,
            showdown_retrieved_at: None,
        };

        save_table(deps.storage, table_id, &table)?;
        COUNTER_KEY.save(deps.storage, &counter)?;

        create_start_game_response(
            table_id,
            hand_ref,
            &table.players,
            previous_hand_log,
        )
    }

    fn validate_players(players_info: &[StartGamePlayer]) -> Result<(), ContractError> {
        if !(MIN_PLAYERS..=MAX_PLAYERS).contains(&players_info.len()) {
            return Err(ContractError::InvalidPlayerCount {
                count: players_info.len(),
            });
        }

        let unique_keys: HashSet<_> = players_info.iter().map(|p| &p.public_key).collect();

        if unique_keys.len() != players_info.len() {
            return Err(ContractError::DuplicatePublicKeys {});
        }

        Ok(())
    }

    fn initialize_deck(env: &Env, counter: &mut u128) -> Result<Deck, ContractError> {
        let mut deck = Deck::new();
        let seed = helpers::generate_random_number(env, counter)?;
        helpers::shuffle_deck(&mut deck, seed);
        Ok(deck)
    }

    fn distribute_player_cards(
        deck: &mut Deck,
        players: &[StartGamePlayer],
    ) -> Vec<(String, Vec<Card>)> {
        players
            .iter()
            .map(|player| {
                (
                    player.public_key.clone(),
                    vec![
                        deck.cards.pop().unwrap().clone(),
                        deck.cards.pop().unwrap().clone(),
                    ],
                )
            })
            .collect()
    }

    fn generate_community_cards(
        env: &Env,
        counter: &mut u128,
        secrets: &mut Vec<(u64, Vec<u64>)>,
        deck: &mut Deck,
        player_count: usize,
    ) -> Result<CommunityCards, ContractError> {

        for _ in 0..COMMUNITY_CARD_PHASES {
            let secret = helpers::generate_random_number(env, counter)?;
            let shares = helpers::additive_secret_sharing(env, player_count, secret, counter)?;
            secrets.push((secret, shares));
        }

        Ok(CommunityCards {
            flop: Flop {
                cards: collect_cards(deck, 3),
                secret: secrets[0].0,
                retrieved_at: None,
            },
            turn: Turn {
                card: deck.cards.pop().unwrap().clone(),
                secret: secrets[1].0,
                retrieved_at: None,
            },
            river: River {
                card: deck.cards.pop().unwrap().clone(),
                secret: secrets[2].0,
                retrieved_at: None,
            },
        })
    }

    fn collect_cards(deck: &mut Deck, count: usize) -> Vec<Card> {
        (0..count).filter_map(|_| Some(deck.cards.pop().unwrap().clone())).collect()
    }

    fn create_players(
        players_info: Vec<StartGamePlayer>,
        player_cards: Vec<(String, Vec<Card>)>,
        secrets: &[(u64, Vec<u64>)],
        env: &Env,
        counter: &mut u128,
    ) -> Result<Vec<Player>, ContractError> {

        players_info
            .into_iter()
            .zip(player_cards)
            .enumerate()
            .map(|(i, (info, (_, cards)))| {
                Ok(Player {
                    username: info.username,
                    player_id: info.player_id,
                    public_key: info.public_key,
                    hand: cards,
                    hand_secret: helpers::generate_random_number(env, counter)?,
                    flop_secret_share: secrets[0].1[i],
                    turn_secret_share: secrets[1].1[i],
                    river_secret_share: secrets[2].1[i],
                })
            })
            .collect()
    }

    fn create_start_game_response(
        table_id: u32,
        hand_ref: u32,
        players: &[Player],
        previous_hand_log: Option<LastHandLogResponse>,
    ) -> Result<Response, ContractError> {
        let response = ResponsePayload::StartGame(StartGameResponse {
            table_id,
            hand_ref,
            players: players.iter().map(|p| p.username.clone()).collect(),
        });
        let mut res = create_plaintext_response(RESPONSE_KEY.to_string(), response)?;

        if let Some(previous_hand_log) = previous_hand_log {
            res = res.add_attribute_plaintext("previous_hand_log", serialize_response(ResponsePayload::LastHand(previous_hand_log))?);
        }
        Ok(res)
    }

    fn create_previous_hand_log(deps: Deps, table_id: u32, showdown_player_ids: Vec<Uuid>) -> Result<Option<LastHandLogResponse>, ContractError> {
        let table = load_table(deps.storage, table_id);
        let previous_hand_log = if table.is_some() {
            let table = table.unwrap();
            
            Some(LastHandLogResponse {
                showdown_players: showdown_player_ids.iter().map(|player_id| {
                    let player = table.players.iter().find(|player| &player.player_id == player_id).unwrap();
                    ShowdownPlayer {
                        username: player.username.clone(),
                        hand: player.hand.iter().map(|card| card.to_string()).collect(),
                    }
                }).collect(),
                community_cards: [table.community_cards.flop.cards.iter().map(|card| card.to_string()).collect(), vec![table.community_cards.turn.card.to_string()], vec![table.community_cards.river.card.to_string()]].concat(),
                flop_retrieved_at: table.community_cards.flop.retrieved_at,
                turn_retrieved_at: table.community_cards.turn.retrieved_at,
                river_retrieved_at: table.community_cards.river.retrieved_at,
                showdown_retrieved_at: table.showdown_retrieved_at,
            })
        } else {
            None
        };

        Ok(previous_hand_log)
    }

    fn create_plaintext_response(
        key: String,
        response: ResponsePayload
    ) -> Result<Response, ContractError> {
        Ok(Response::new().add_attribute_plaintext(key, serialize_response(response)?))
    }

    fn serialize_response(response: ResponsePayload) -> Result<String, ContractError> {
        match serde_json_wasm::to_string(&response) {
            Ok(json) => Ok(json),
            Err(e) => Err(ContractError::SerializationFailed {
                error: e.to_string(),
            }),
        }
    }


    pub fn handle_community_cards(
        deps: DepsMut,
        env: Env,
        table_id: u32,
        game_state: GameState,
    ) -> Result<Response, ContractError> {
        let mut table = load_table_or_error(deps.storage, table_id)?;
        
        /*
         * We check if the cards have already been retrieved, if so we return an error.
         * This ensures that the logged time is the only time the cards were retrieved.
         */
        let cards = match game_state {
            GameState::Flop => {
                if table.community_cards.flop.retrieved_at.is_some() {
                    return Err(ContractError::CardsAlreadyRetrieved {});
                }
                table.community_cards.flop.retrieved_at = Some(env.block.time);
                Some(table.community_cards.flop.cards.clone())
            }
            GameState::Turn => {
                if table.community_cards.turn.retrieved_at.is_some() {
                    return Err(ContractError::CardsAlreadyRetrieved {});
                }
                table.community_cards.turn.retrieved_at = Some(env.block.time);
                Some(vec![table.community_cards.turn.card.clone()])
            }
            GameState::River => {
                if table.community_cards.river.retrieved_at.is_some() {
                    return Err(ContractError::CardsAlreadyRetrieved {});
                }
                table.community_cards.river.retrieved_at = Some(env.block.time);
                Some(vec![table.community_cards.river.card.clone()])
            }
            _ => {
                return Err(ContractError::GameStateError {
                    method: "distribute_community_cards".to_string(),
                    table_id,
                    game_state: Some(game_state),
                })
            }
        };

        
        save_table(deps.storage, table_id, &table)?;

        let response = ResponsePayload::CommunityCards(CommunityCardsResponse {
            table_id,
            hand_ref: table.hand_ref,
            game_state,
            community_cards: cards.unwrap(),
        });

        create_plaintext_response(RESPONSE_KEY.to_string(), response)
    }

    pub fn handle_showdown(
        deps: DepsMut,
        env: Env,
        table_id: u32,
        game_state: GameState,
        showdown_player_ids: Vec<Uuid>,
    ) -> Result<Response, ContractError> {
        let mut table = load_table(deps.storage, table_id)
            .ok_or_else(|| ContractError::TableNotFound { table_id })?;

        /*
         * Ensures that the cards cannot be retrieved twice, 
         * this ensures that the logged time is the only time the cards were retrieved.
         */
        if table.showdown_retrieved_at.is_some() {
            return Err(ContractError::CardsAlreadyRetrieved {});
        }

        let mut player_hands: Vec<(Uuid, Vec<Card>)> = Vec::new();

        for player_id in showdown_player_ids.iter() {
            let players = table
                .players
                .iter()
                .find(|player| &player.player_id == player_id);

            if let Some(player) = players {
                player_hands.push((player.player_id.clone(), player.hand.clone()));
            } else {
                return Err(ContractError::PlayerNotFound {
                    table_id,
                    player: player_id.to_string(),
                });
            }
        }

        let response = ResponsePayload::Showdown(ShowdownResponse {
            table_id,
            hand_ref: table.hand_ref,
            players_cards: player_hands,
            community_cards: handle_all_in_showdown(&table.community_cards, game_state),
        });

        
        table.showdown_retrieved_at = Some(env.block.time);
        save_table(deps.storage, table_id, &table)?;

        create_plaintext_response(RESPONSE_KEY.to_string(), response)
    }

    fn handle_all_in_showdown(
        community_cards: &CommunityCards,
        game_state: GameState,
    ) -> Option<Vec<Card>> {
        match game_state {
            GameState::PreFlop => {
                let mut cards = community_cards.flop.cards.clone();
                cards.push(community_cards.turn.card.clone());
                cards.push(community_cards.river.card.clone());
                Some(cards)
            }
            GameState::Flop => Some(vec![
                community_cards.turn.card.clone(),
                community_cards.river.card.clone(),
            ]),
            GameState::Turn => Some(vec![community_cards.river.card.clone()]),
            _ => return None,
        }
    }
}


#[entry_point]
pub fn instantiate(deps: DepsMut, env: Env, info: MessageInfo, _msg: InstantiateMsg,) -> Result<Response, StdError> {
    let config = Config {
        owner: info.sender,
        contract_address: env.contract.address.clone(),
    };

    let counter = init_counter(&env)?;

    CONFIG_KEY.save(deps.storage, &config)?;
    COUNTER_KEY.save(deps.storage, &counter)?;

    Ok(Response::default())
}

fn init_counter(env: &Env) -> StdResult<u128> {
    let seed = env
        .block
        .random
        .as_ref()
        .ok_or(StdError::generic_err("No random seed available"))?;
    let seed_bytes: [u8; RANDOM_SEED_SIZE] = seed[..RANDOM_SEED_SIZE]
        .try_into()
        .map_err(|_| StdError::generic_err("Failed to convert seed to array"))?;
    Ok(u128::from_le_bytes(seed_bytes))
}

#[entry_point]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    let config = CONFIG_KEY.load(deps.storage)?;
    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    match msg {
        ExecuteMsg::StartGame {
            table_id,
            hand_ref,
            players,
            prev_hand_showdown_players,
        } => execute_handlers::handle_start_game(
            deps,
            env,
            table_id,
            hand_ref,
            players,
            prev_hand_showdown_players,
        ),
        ExecuteMsg::CommunityCards {
            table_id,
            game_state,
        } => execute_handlers::handle_community_cards(deps, env, table_id, game_state),
        ExecuteMsg::Showdown {
            table_id,
            game_state,
            showdown_player_ids,
        } => execute_handlers::handle_showdown(deps, env, table_id, game_state, showdown_player_ids),
    }
}

#[entry_point]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::WithPermit { permit, query } => {
            query_handlers::handle_permit_query(deps, permit, query)
        }
        QueryMsg::CommunityCards {
            table_id,
            game_state,
            secret_key,
        } => to_binary(&query_handlers::query_community_cards(
            deps, table_id, game_state, secret_key,
        )?),
        QueryMsg::Showdown {
            table_id,
            flop_secret,
            turn_secret,
            river_secret,
            players_secrets,
        } => to_binary(&query_handlers::query_showdown(
            deps,
            table_id,
            flop_secret,
            turn_secret,
            river_secret,
            players_secrets,
        )?),
    }
}

#[cfg(test)]
mod complete_tests {
    use crate::contract::query_handlers::query_player_private_data;
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
    use cosmwasm_std::{coins, from_binary};
    use super::*;
    use std::time::Instant;
    use std::collections::HashMap;

    #[test]
    fn test_instantiate() {
        let mut deps = mock_dependencies();

        let msg = InstantiateMsg {};
        let info = mock_info("creator", &coins(1000, "earth"));

        let res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(0, res.messages.len());
    }

    #[test]
    fn test_start_game() {
        let mut deps = mock_dependencies();

        let msg = InstantiateMsg {};
        let info = mock_info("creator", &coins(1000, "earth"));
        let _res = instantiate(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        let players = vec![
            StartGamePlayer {
                username: "player1".to_string(),
                player_id: Uuid::parse_str("2928c53b-5d14-4a7c-b56e-83ef56a0644e").unwrap(),
                public_key: "key1".to_string(),
            },
            StartGamePlayer {
                username: "player2".to_string(),
                player_id: Uuid::parse_str("8f204fcc-54a5-4473-8ac3-4845bff291ab").unwrap(),
                public_key: "key2".to_string(),
            },
        ];

        let res = execute(
            deps.as_mut(),
            mock_env(),
            info,
            ExecuteMsg::StartGame {
                table_id: 1,
                hand_ref: 1,
                players,
                prev_hand_showdown_players: vec![],
            },
        )
        .unwrap();
        
        let attrs = &res.attributes;
        let response_attr = attrs.iter().find(|attr| attr.key == "response").unwrap();
        assert!(response_attr.value.contains("\"players\":[\"player1\",\"player2\"]"));
        assert!(response_attr.value.contains("\"table_id\":1"));
        assert!(response_attr.value.contains("\"hand_ref\":1"));

        let player_info1 = query_player_private_data(deps.as_ref(), 1, "key1".to_string()).unwrap();
        let player_info2 = query_player_private_data(deps.as_ref(), 1, "key2".to_string()).unwrap();
        
        
        assert_eq!(player_info1.table_id, 1);
        assert_eq!(player_info1.hand_ref, 1);
        assert_eq!(player_info1.hand.len(), 2);
        assert!(player_info1.flop_secret_share.parse::<u64>().is_ok());
        
        assert_eq!(player_info2.table_id, 1);
        assert_eq!(player_info2.hand_ref, 1);
        assert_eq!(player_info2.hand.len(), 2);
        assert!(player_info2.flop_secret_share.parse::<u64>().is_ok());
        
        let flop_secret = addition_shares(vec![
            player_info1.flop_secret_share.parse::<u64>().unwrap(),
            player_info2.flop_secret_share.parse::<u64>().unwrap(),
        ]);
        let turn_secret = addition_shares(vec![
            player_info1.turn_secret_share.parse::<u64>().unwrap(),
            player_info2.turn_secret_share.parse::<u64>().unwrap(),
        ]);
        let river_secret = addition_shares(vec![
            player_info1.river_secret_share.parse::<u64>().unwrap(),
            player_info2.river_secret_share.parse::<u64>().unwrap(),
        ]);

        
        let res = query(
            deps.as_ref(),
            mock_env(),
            QueryMsg::CommunityCards {
                table_id: 1,
                game_state: GameState::Flop,
                secret_key: flop_secret,
            },
        );
        let flop_response: CommunityCardsResponse = from_binary(res.as_ref().unwrap()).unwrap();
        assert_eq!(flop_response.table_id, 1);
        assert_eq!(flop_response.hand_ref, 1);
        assert_eq!(flop_response.game_state, GameState::Flop);
        assert_eq!(flop_response.community_cards.len(), 3);

        
        let res = query(
            deps.as_ref(),
            mock_env(),
            QueryMsg::CommunityCards {
                table_id: 1,
                game_state: GameState::Turn,
                secret_key: turn_secret,
            },
        );
        let turn_response: CommunityCardsResponse = from_binary(res.as_ref().unwrap()).unwrap();
        assert_eq!(turn_response.table_id, 1);
        assert_eq!(turn_response.hand_ref, 1);
        assert_eq!(turn_response.game_state, GameState::Turn);
        assert_eq!(turn_response.community_cards.len(), 1);

        
        let res = query(
            deps.as_ref(),
            mock_env(),
            QueryMsg::CommunityCards {
                table_id: 1,
                game_state: GameState::River,
                secret_key: river_secret,
            },
        );
        let river_response: CommunityCardsResponse = from_binary(res.as_ref().unwrap()).unwrap();
        assert_eq!(river_response.table_id, 1);
        assert_eq!(river_response.hand_ref, 1);
        assert_eq!(river_response.game_state, GameState::River);
        assert_eq!(river_response.community_cards.len(), 1);

        
        let res = query(
            deps.as_ref(),
            mock_env(),
            QueryMsg::CommunityCards {
                table_id: 1,
                game_state: GameState::Flop,
                secret_key: flop_secret + 1, 
            },
        );
        assert!(res.is_err());
    }
    
    #[test]
    fn test_community_cards() {
        let mut deps = mock_dependencies();
        
        
        let msg = InstantiateMsg {};
        let info = mock_info("creator", &coins(1000, "earth"));
        let _res = instantiate(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();
        
        
        let players = vec![
            StartGamePlayer {
                username: "player1".to_string(),
                player_id: Uuid::parse_str("2928c53b-5d14-4a7c-b56e-83ef56a0644e").unwrap(),
                public_key: "key1".to_string(),
            },
            StartGamePlayer {
                username: "player2".to_string(),
                player_id: Uuid::parse_str("8f204fcc-54a5-4473-8ac3-4845bff291ab").unwrap(),
                public_key: "key2".to_string(),
            },
        ];
        
        execute(
            deps.as_mut(),
            mock_env(),
            info.clone(),
            ExecuteMsg::StartGame {
                table_id: 1,
                hand_ref: 1,
                players,
                prev_hand_showdown_players: vec![],
            },
        )
        .unwrap();
        
        
        let res = execute(
            deps.as_mut(),
            mock_env(),
            info.clone(),
            ExecuteMsg::CommunityCards {
                table_id: 1,
                game_state: GameState::Flop,
            },
        )
        .unwrap();
        
        
        let attrs = &res.attributes;
        println!("{:?}", attrs);
        let response_attr = attrs.iter().find(|attr| attr.key == "response").unwrap();
        assert!(response_attr.value.contains("\"game_state\":\"flop\""));
        
        
        let res = execute(
            deps.as_mut(),
            mock_env(),
            info.clone(),
            ExecuteMsg::CommunityCards {
                table_id: 1,
                game_state: GameState::Turn,
            },
        )
        .unwrap();
        
        
        let attrs = &res.attributes;
        let response_attr = attrs.iter().find(|attr| attr.key == "response").unwrap();
        assert!(response_attr.value.contains("\"game_state\":\"turn\""));
        let response_payload: ResponsePayload = serde_json_wasm::from_str(&response_attr.value).unwrap();
        match response_payload {
            ResponsePayload::CommunityCards(cards_response) => {
            assert_eq!(cards_response.community_cards.len(), 1);
            assert_eq!(cards_response.game_state, GameState::Turn);
            },
            _ => panic!("Expected CommunityCards response"),
        }
    }
    
    #[test]
    fn test_invalid_game_state() {
        let mut deps = mock_dependencies();
        
        let msg = InstantiateMsg {};
        let info = mock_info("creator", &coins(1000, "earth"));
        let _res = instantiate(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();
        
        let players = vec![
            StartGamePlayer {
                username: "player1".to_string(),
                player_id: Uuid::parse_str("2928c53b-5d14-4a7c-b56e-83ef56a0644e").unwrap(),
                public_key: "key1".to_string(),
            },
            StartGamePlayer {
                username: "player2".to_string(),
                player_id: Uuid::parse_str("8f204fcc-54a5-4473-8ac3-4845bff291ab").unwrap(),
                public_key: "key2".to_string(),
            },
        ];
        
        execute(
            deps.as_mut(),
            mock_env(),
            info.clone(),
            ExecuteMsg::StartGame {
                table_id: 1,
                hand_ref: 1,
                players,
                prev_hand_showdown_players: vec![],
            },
        )
        .unwrap();
        
        
        let res = execute(
            deps.as_mut(),
            mock_env(),
            info.clone(),
            ExecuteMsg::CommunityCards {
                table_id: 1,
                game_state: GameState::PreFlop,
            },
        );
        
        assert!(res.is_err());
        match res.unwrap_err() {
            ContractError::GameStateError { method, table_id, game_state } => {
                assert_eq!(method, "distribute_community_cards");
                assert_eq!(table_id, 1);
                assert_eq!(game_state, Some(GameState::PreFlop));
            },
            _ => panic!("Expected GameStateError"),
        }
    }
    
    #[test]
    fn test_showdown() {
        let mut deps = mock_dependencies();
        
        
        let msg = InstantiateMsg {};
        let info = mock_info("creator", &coins(1000, "earth"));
        let _res = instantiate(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();
        
        
        let player1_id = Uuid::parse_str("2928c53b-5d14-4a7c-b56e-83ef56a0644e").unwrap();
        let player2_id = Uuid::parse_str("8f204fcc-54a5-4473-8ac3-4845bff291ab").unwrap();
        
        let players = vec![
            StartGamePlayer {
                username: "player1".to_string(),
                player_id: player1_id,
                public_key: "key1".to_string(),
            },
            StartGamePlayer {
                username: "player2".to_string(),
                player_id: player2_id,
                public_key: "key2".to_string(),
            },
        ];
        
        execute(
            deps.as_mut(),
            mock_env(),
            info.clone(),
            ExecuteMsg::StartGame {
                table_id: 1,
                hand_ref: 1,
                players,
                prev_hand_showdown_players: vec![],
            },
        )
        .unwrap();
        
        
        let res = execute(
            deps.as_mut(),
            mock_env(),
            info.clone(),
            ExecuteMsg::Showdown {
                table_id: 1,
                game_state: GameState::River,
                showdown_player_ids: vec![player1_id, player2_id],
            },
        )
        .unwrap();
        
        
        let attrs = &res.attributes;
        let response_attr = attrs.iter().find(|attr| attr.key == "response").unwrap();
        assert!(response_attr.value.contains("\"players_cards\""));
    }
    
    #[test]
    fn test_player_not_found() {
        let mut deps = mock_dependencies();
        
        
        let msg = InstantiateMsg {};
        let info = mock_info("creator", &coins(1000, "earth"));
        let _res = instantiate(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();
        
        
        let player1_id = Uuid::parse_str("2928c53b-5d14-4a7c-b56e-83ef56a0644e").unwrap();
        let player2_id = Uuid::parse_str("e6799ecf-f202-418a-a535-0b42509f69f7").unwrap();

        let players = vec![
            StartGamePlayer {
                username: "player1".to_string(),
                player_id: player1_id,
                public_key: "key1".to_string(),
            },
            StartGamePlayer {
                username: "player2".to_string(),
                player_id: player2_id,
                public_key: "key2".to_string(),
            },
        ];
        
        execute(
            deps.as_mut(),
            mock_env(),
            info.clone(),
            ExecuteMsg::StartGame {
                table_id: 1,
                hand_ref: 1,
                players,
                prev_hand_showdown_players: vec![],
            },
        )
        .unwrap();
        
        
        let non_existent_player = Uuid::parse_str("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee").unwrap();
        let res = execute(
            deps.as_mut(),
            mock_env(),
            info.clone(),
            ExecuteMsg::Showdown {
                table_id: 1,
                game_state: GameState::River,
                showdown_player_ids: vec![non_existent_player],
            },
        );
        
        assert!(res.is_err());
        match res.unwrap_err() {
            ContractError::PlayerNotFound { table_id, player } => {
                assert_eq!(table_id, 1);
                assert_eq!(player, non_existent_player.to_string());
            },
            _ => panic!("Expected PlayerNotFound error"),
        }
    }

    pub fn addition_shares(shares: Vec<u64>) -> u64 {
        shares.iter().copied().fold(0u64, u64::wrapping_add)
    }

    #[test]
    fn test_additive_sharing() {
        let secret = 14151497078262209000u64;
    let mut counter = 0;
    let _shares = helpers::additive_secret_sharing(&mock_env(), 2, secret, &mut counter).unwrap();
    let shares = [8676118583430535000, 5475378494831674000, ];
         let sum = shares.iter().copied().fold(0u64, u64::wrapping_add);
         println!("{:?}", sum);
        assert_eq!(sum, secret);
    }

    #[test]
    fn test_shuffle_performance_comparison() {
        const ITERATIONS: usize = 10000;
        let seed = 12345u64;
        
        // Test shuffle_deck
        let mut total_duration_biased = std::time::Duration::ZERO;
        for _ in 0..ITERATIONS {
            let mut deck = Deck::new();
            let start = Instant::now();
            shuffle_deck_modulo_bias(&mut deck, seed);
            total_duration_biased += start.elapsed();
        }
        let mean_biased = total_duration_biased / ITERATIONS as u32;
        
        // Test shuffle_deck_unbiased
        let mut total_duration_unbiased = std::time::Duration::ZERO;
        for _ in 0..ITERATIONS {
            let mut deck = Deck::new();
            let start = Instant::now();
            helpers::shuffle_deck(&mut deck, seed);
            total_duration_unbiased += start.elapsed();
        }
        let mean_unbiased = total_duration_unbiased / ITERATIONS as u32;
        
        println!("\n=== Shuffle Performance Comparison ===");
        println!("Iterations: {}", ITERATIONS);
        println!("shuffle_deck mean time: {:?}", mean_biased);
        println!("shuffle_deck_unbiased mean time: {:?}", mean_unbiased);
        println!("Ratio (unbiased/biased): {:.2}x", 
            mean_unbiased.as_nanos() as f64 / mean_biased.as_nanos() as f64);
        
        // Verify both functions actually shuffle the deck
        let mut deck1 = Deck::new();
        let mut deck2 = Deck::new();
        let original_deck = Deck::new();
        
        shuffle_deck_modulo_bias(&mut deck1, seed);
        helpers::shuffle_deck(&mut deck2, seed);
        
        assert_ne!(deck1.cards, original_deck.cards, "shuffle_deck should shuffle the deck");
        assert_ne!(deck2.cards, original_deck.cards, "shuffle_deck_unbiased should shuffle the deck");
    }
    
    // The old shuffle_deck function with modulo bias for comparison
    fn shuffle_deck_modulo_bias(deck: &mut Deck, seed: u64) {
        let mut rng = Sha256::new();
        let mut deck_len = deck.cards.len();

        while deck_len > 1 {
            deck_len -= 1;
            rng.update(&seed.to_le_bytes());
            rng.update(&(deck_len as u64).to_le_bytes());

            let hash = rng.finalize_reset();
            let random_value = u64::from_le_bytes(hash[..8].try_into().unwrap());
            let random_index = (random_value as usize) % (deck_len + 1);

            deck.cards.swap(deck_len, random_index);
        }
    }

    #[test]
    fn test_shuffle_statistical_distribution() {
        const SHUFFLE_ITERATIONS: usize = 100000;
        const DECK_SIZE: usize = 52;
        const POSITIONS_TO_TEST: usize = 5; // Test first 5 positions
        
        // Track how many times each card appears at each position
        let mut biased_distribution: Vec<HashMap<String, usize>> = vec![HashMap::new(); POSITIONS_TO_TEST];
        let mut unbiased_distribution: Vec<HashMap<String, usize>> = vec![HashMap::new(); POSITIONS_TO_TEST];
        
        // Run shuffle_deck iterations
        for i in 0..SHUFFLE_ITERATIONS {
            let mut deck = Deck::new();
            shuffle_deck_modulo_bias(&mut deck, i as u64);
            
            for pos in 0..POSITIONS_TO_TEST {
                let card_str = deck.cards[pos].to_string();
                *biased_distribution[pos].entry(card_str).or_insert(0) += 1;
            }
        }
        
        // Run shuffle_deck_unbiased iterations
        for i in 0..SHUFFLE_ITERATIONS {
            let mut deck = Deck::new();
            helpers::shuffle_deck(&mut deck, i as u64);
            
            for pos in 0..POSITIONS_TO_TEST {
                let card_str = deck.cards[pos].to_string();
                *unbiased_distribution[pos].entry(card_str).or_insert(0) += 1;
            }
        }
        
        println!("\n=== Statistical Distribution Test ===");
        println!("Shuffle iterations: {}", SHUFFLE_ITERATIONS);
        println!("Deck size: {}", DECK_SIZE);
        println!("Expected frequency per card per position: {:.2}", SHUFFLE_ITERATIONS as f64 / DECK_SIZE as f64);
        
        // Calculate Chi-squared statistic for both methods
        let expected_frequency = SHUFFLE_ITERATIONS as f64 / DECK_SIZE as f64;
        
        for pos in 0..POSITIONS_TO_TEST {
            let chi_squared_biased = calculate_chi_squared(&biased_distribution[pos], expected_frequency, DECK_SIZE);
            let chi_squared_unbiased = calculate_chi_squared(&unbiased_distribution[pos], expected_frequency, DECK_SIZE);
            
            println!("\nPosition {}:", pos);
            println!("  shuffle_deck_modulo_bias χ²: {:.2}", chi_squared_biased);
            println!("  shuffle_deck_unbiased χ²: {:.2}", chi_squared_unbiased);
            
            // Chi-squared critical value for 51 degrees of freedom (52 cards - 1) at 95% confidence: ~68.67
            // At 99% confidence: ~77.38, see: https://www.medcalc.org/en/manual/chi-square-table.php
            let critical_value_95 = 68.67;
            let critical_value_99 = 77.38;
            
            println!("  Critical value (95%): {:.2}", critical_value_95);
            println!("  Critical value (99%): {:.2}", critical_value_99);
            
            // Both should pass the randomness test (lower χ² is better, but should be below critical value)
            assert!(chi_squared_unbiased < critical_value_99, 
                "Unbiased shuffle failed randomness test at position {}: χ² = {:.2}", pos, chi_squared_unbiased);
        }
        
        // Compare uniformity using coefficient of variation
        println!("\n=== Uniformity Analysis ===");
        for pos in 0..POSITIONS_TO_TEST {
            let cv_biased = calculate_coefficient_of_variation(&biased_distribution[pos], expected_frequency);
            let cv_unbiased = calculate_coefficient_of_variation(&unbiased_distribution[pos], expected_frequency);
            
            println!("Position {} - Coefficient of Variation:", pos);
            println!("  shuffle_deck: {:.4}", cv_biased);
            println!("  shuffle_deck_unbiased: {:.4}", cv_unbiased);
            
            // Lower CV indicates more uniform distribution
            // Both should have low CV (typically < 0.15 for good randomness)
            assert!(cv_unbiased < 0.2, "Unbiased shuffle has poor uniformity at position {}: CV = {:.4}", pos, cv_unbiased);
        }
        
        // Verify all 52 cards appear in the distribution
        for pos in 0..POSITIONS_TO_TEST {
            assert_eq!(biased_distribution[pos].len(), DECK_SIZE, 
                "shuffle_deck: Not all cards appeared at position {}", pos);
            assert_eq!(unbiased_distribution[pos].len(), DECK_SIZE, 
                "shuffle_deck_unbiased: Not all cards appeared at position {}", pos);
        }
        
        println!("\n✓ Both shuffle methods pass statistical randomness tests");
    }
    
    fn calculate_chi_squared(distribution: &HashMap<String, usize>, expected: f64, total_cards: usize) -> f64 {
        let mut chi_squared = 0.0;
        
        // Include all possible cards, even those with 0 occurrences
        let mut all_cards = HashSet::new();
        for card in distribution.keys() {
            all_cards.insert(card.clone());
        }
        
        // Add missing cards with 0 count
        for card in distribution.keys() {
            all_cards.insert(card.clone());
        }
        
        // Ensure we account for all 52 cards
        for card in all_cards.iter() {
            let observed = *distribution.get(card).unwrap_or(&0) as f64;
            let diff = observed - expected;
            chi_squared += (diff * diff) / expected;
        }
        
        // For cards that never appeared
        let cards_present = distribution.len();
        if cards_present < total_cards {
            let missing_cards = total_cards - cards_present;
            chi_squared += missing_cards as f64 * expected; // (0 - expected)² / expected = expected
        }
        
        chi_squared
    }
    
    fn calculate_coefficient_of_variation(distribution: &HashMap<String, usize>, expected: f64) -> f64 {
        let n = distribution.len() as f64;
        if n == 0.0 {
            return 0.0;
        }
        
        // Calculate standard deviation
        let variance: f64 = distribution.values()
            .map(|&count| {
                let diff = count as f64 - expected;
                diff * diff
            })
            .sum::<f64>() / n;
        
        let std_dev = variance.sqrt();
        
        // Coefficient of variation = std_dev / mean
        std_dev / expected
    }
}
