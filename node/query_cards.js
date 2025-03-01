import { PlayerDataResponse } from "./msg-models/player-data-response.js";
import {community_cards_query, showdown_query, random,playerTest, clientTest, client2, chainId,  player2, player3,createSecretNetworkClient, player, loadContractInfo, contractInfo, trx, start_game, flop, showdown_all_in, client1, execute, turn, river, showdown} from "./shared.js";

let permitName = "query_cards";
let allowedTokens = [contractInfo.contractAddress];
let permissions = ["allowance"];

let getSignature =  async (wallet) => {
    const { signature } = await wallet.signAmino(
        wallet.address,
        {
        chain_id: chainId,
        account_number: "0", // Must be 0
        sequence: "0", // Must be 0
        fee: {
            amount: [{ denom: "uscrt", amount: "0" }], // Must be 0 uscrt
            gas: "1", // Must be 1
        },
        msgs: [
            {
            type: "query_permit", // Must be "query_permit"
            value: {
                permit_name: permitName,
                allowed_tokens: allowedTokens,
                permissions: permissions,
            },
            },
        ],
        memo: "", // Must be empty
        },
        {
        preferNoSetFee: true, // Fee must be 0, so hide it from the user
        preferNoSetMemo: true, // Memo must be empty, so hide it from the user
        }
    );
    return signature;
    };

let get_player_info = async (secretjs, signature) => {
  const res = await secretjs.query.compute.queryContract(
    {
      contract_address: contractInfo.contractAddress,
      code_hash: contractInfo.contractCodeHash,
      query: {
        with_permit: {
          query: { player_private_data: {table_id: 999} },
          permit: {
            params: {
              permit_name: permitName,
              allowed_tokens: allowedTokens,
              chain_id: chainId,
              permissions: permissions,
            },
            signature: signature,
          },
        },
      },
    },
  );

  return res;
};

async function calls(client, signature) {
  const res = await get_player_info(client, signature);
  console.log(res);
  const parsed = PlayerDataResponse.fromJson(res);
  console.log(parsed);
}
let signature2 = await getSignature(player2.wallet);
let signature3 = await getSignature(player3.wallet);

function parseLargeNumbers(jsonString) {
  return JSON.parse(jsonString, (key, value) => {
    // Convert large numbers to BigInt
    if (typeof value === 'string' && /^\d+$/.test(value)) {
      return BigInt(value);
    }
    return value;
  });
}

const player_info2 = parseLargeNumbers(await get_player_info(client2, signature2));
const player_info3 = parseLargeNumbers(await get_player_info(client2, signature3));

console.log(player_info2);
let query = async (secretjs, msg) => {
  const res = await secretjs.query.compute.queryContract(
    {
      contract_address: contractInfo.contractAddress,
      code_hash: contractInfo.contractCodeHash,
      query: {
      ...msg,
      },
    },
  );

  return res;
};

function wrappingAdd(a, b) {
  const mask = (1n << 64n) - 1n; // Masque pour 64 bits (2^64 - 1)
  return (a + b) & mask; // Appliquer le masque pour simuler le wrapping
}

function additionShares(shares) {
  return shares.reduce((sum, share) => wrappingAdd(sum, BigInt(share)), 0n);
}

const additiveSecret = additionShares([BigInt(player_info2.flop_secret_share), BigInt(player_info3.flop_secret_share)]);
// 4499291295878049100;
console.log(additiveSecret.toString());
const msg = showdown_query([player_info2.hand_secret, player_info3.hand_secret], additiveSecret.toString());
try {
  query(client1, msg).then((res) => {
    console.log(res);
  });
} catch (error) {
  console.error("Error querying contract:", error);
}
