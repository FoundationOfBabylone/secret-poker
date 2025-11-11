# Secure Crypto Poker Platform Overview

## User-Focused Summary

Our platform provides a trustworthy online poker experience where players can enjoy games using cryptocurrency while maintaining complete confidence in the game's fairness. Here's what we offer:

**Complete Transparency:** Through innovative use of secret contracts on the Secret Network blockchain, every hand is demonstrably fair. Players can verify that card dealing is truly random and that the platform cannot manipulate outcomes.

**Foolproof Authentication:** Our system uses cryptographic signatures through the SNIP-20 protocol, ensuring only verified users can access their game data.

**Seamless Experience:** Despite using a 6s block-time chain behind the scenes, the games are fluid and actions responsive, with minimal alteration of the user experience.

## Development Deep Dive

### Evolution of Our Approach

The functional objective — successfully achieved — was to develop a smart contract that retains full authority over card generation and distribution throughout the game.

Our initial design relied on publicly logged contract executions to retrieve card data for each game phase. While technically functional, this approach resulted in poor user experience due to ~6 second response times. 

We've since evolved to a more sophisticated architecture that executes the contract only once at game start and uses efficient queries for subsequent game phases. This solution leverages [Additive Secret Sharing](https://en.wikipedia.org/wiki/Secret_sharing) method combined with Secret Network's SNIP-20 permit queries. The original slower approach remains implemented as a fallback mechanism when the query approach is unavailable.

![Image Description](./POKER_FLOW_CHART_V4_compressed.jpg)

### Simplified Process Flow

#### Game Hand Start Process

1. **Secure Randomization**: We generate truly random outcomes using a random number generator (RNG) seeded with Secret Network's Verifiable Random Function (VRF). This creates:
   - A shuffled deck
   - Three "Game-secrets" (one for each game phase: flop, turn, river)
   - Individual "Hand-secrets" for each player's cards

2. **Secret Distribution**: Using the Additive Secret Sharing method, each Game-secret is split into multiple "shares" distributed among players. The original Game-secret can only be reconstructed when all shares are combined

3. **Secure Retrieval**: On the client side, players access their:
   - Portion of Game-secret shares
   - Personal Hand-secret
   - Hand cards
   
   This is accomplished by signing a cryptographic permit with their locally generated/stored private key

4. **Identity Verification**: The contract validates each signature against the corresponding public key, ensuring only legitimate players can access their assigned information

#### Flop/Turn/River Handling

* **Standard Operation**: During normal gameplay, all players contribute their shares of the Game-secrets for each relevant phase
* **Fallback Mechanism**: If players are missing (disconnected, etc.), the system reverts to the contract execution method to retrieve cards, ensuring game continuity

#### Showdown Mechanism

* **Hand Revelation**: At showdown, players submit their Hand-secrets to the backend
* **Winner Determination**: The backend queries the contract with these relevant keys to accurately determine winners according to standard poker rules

This hybrid approach provides both optimal performance and guaranteed reliability while maintaining the cryptographic security of the game.

## "How can I verify it, be assured that those are just not words ?"

