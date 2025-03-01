import { SecretNetworkClient, Wallet, MsgExecuteContract } from "secretjs";
import * as fs from "fs";
export const loadContractInfo = (contractInfoPath) => {
  let contractInfo;
  
  if (fs.existsSync(contractInfoPath)) {
    const contractInfoData = fs.readFileSync(contractInfoPath, "utf8");
    contractInfo = JSON.parse(contractInfoData);
    console.log("Contract info loaded:", contractInfo);
  } else {
    console.error("Contract info file not found:", contractInfoPath);
  }
  return contractInfo;
};

export const contractInfo = loadContractInfo("contractInfo.json");
const wallet = new Wallet("desk pigeon hammer sleep only mistake stool december offer patrol once vacant");
const wallet2 = new Wallet("pigeon desk hammer sleep only mistake stool december offer patrol once vacant");
const wallet3 = new Wallet("hammer desk pigeon sleep only mistake stool december offer patrol once vacant");
const walletTest = new Wallet("kid silent piano table caught claw pool robust arrest face choice luxury gesture artist harvest empower canyon hill sadness diesel axis festival jungle phrase");
export const player2 = {
  username: "player2",
  playerId: "3c835440-62b4-4750-946c-f0e622f5cd57",
  wallet: wallet2,
  address: wallet2.address,
};
export const player = {
  username: "player",
  playerId: "520b73c6-c4bf-4374-a63c-6a49b731e1cf",
  wallet: wallet,
  address: wallet.address,
};
export const player3 = {
  username: "player3",
  playerId: "3327ec7f-504e-4283-950a-47d602130d2e",
  wallet: wallet3,
  address: wallet3.address,
};
export const playerTest = {
  username: "playerTest",
  playerId: "3327ec7f-504e-4283-950a-47d602130d2e",
  wallet: walletTest,
  address: walletTest.address,
};

export const chainId = "pulsar-3";

export const createSecretNetworkClient = (player) => {
  return new SecretNetworkClient({
    chainId: chainId,
    url: "https://pulsar.lcd.secretnodes.com",
    wallet: player.wallet,
    walletAddress: player.address,
  });
};

export const client1 = createSecretNetworkClient(player);
export const client2 = createSecretNetworkClient(player2);
export const client3 = createSecretNetworkClient(player3);
export const clientTest = createSecretNetworkClient(walletTest);

export const random = {
  random: {},
}


const playerStartGame = (player) => {
  return {
    username: player.username,
    player_id: player.playerId,
    public_key: player.address,
  }
}

export const start_game = {
    start_game: {
      table_id: 999,
      hand_ref: 1,
      players: [
        playerStartGame(player2),
        playerStartGame(player3),
      ],
      prev_hand_showdown_players: [],
    },
  };

export const community_cards_query = (game_state, secret) => {
  return{ community_cards: {
    table_id: 999,
    game_state: game_state,
    secret_key: secret.toString(),
  }
}
}

export const showdown_query = (players_secrets, flop_secret, turn_secret, river_secret) => {
  return {
    showdown: {
      table_id: 999,
      players_secrets: players_secrets.map(player => player.toString()),
      flop_secret: flop_secret.toString(),
      turn_secret: turn_secret?.toString() ?? null,
      river_secret: river_secret?.toString() ?? null,
    }
  }
}
export const flop = {
    community_cards: {
      table_id: 1,
      game_state: "flop", // "flop" corresponds to 1
    },
  };  

  export const turn = {
    community_cards: {
      table_id: 1,
      game_state: "turn", // "flop" corresponds to 1
    },
  };

  export const river = {
    community_cards: {
      table_id: 1,
      game_state: "river", // "flop" corresponds to 1
    },
  };

  export const showdown_all_in = {
    showdown: {
      table_id: 1,
      show_cards: [
        // wallet.address,
        // wallet2.address,
        // wallet3.address,
      ],
      all_in_showdown: true,
    }
  };

    export const showdown = {
        showdown: {
        table_id: 1,
        show_cards: [
            player.address,
            player2.address,
            player3.address,
        ],
        all_in_showdown: false,
        }
    };

export const execute = async (address, secretjs, info, msg) => {
  try {
    const flip_tx = await secretjs.tx.compute.executeContract(
      {
        sender: address,
        contract_address: info.contractAddress,
        msg: msg,
        code_hash: info.contractCodeHash,
      },
      { gasLimit: 50_000 }
    );
    console.log(flip_tx);
  } catch (error) {
    console.error("Error executing contract:", error);
  }
};

export const trx = (address, info, msg) => {
  let trx = new MsgExecuteContract({
    sender: address,
    contract_address: info.contractAddress,
    msg: msg,
    code_hash: info.contractCodeHash,
  });

  return trx;
};