import {random, client2,  player2, player3,createSecretNetworkClient, player, loadContractInfo, contractInfo, trx, start_game, flop, showdown_all_in, client1, execute, turn, river, showdown} from "./shared.js";
import * as fs from "fs";


// measureTrxTime(wallet.address, contractInfo);
// console.log(wallet3.address);

let broadcast = async (address, secretjs, info, msg) => {
  try {
    const response = await secretjs.tx.broadcast(
      [trx(address, info, msg)],
      { gasLimit: 50_000,
        broadcastMode: "Sync" 
      }
    );
    fs.writeFileSync("response.json", JSON.stringify(response, null, 2));
    console.log(response);
  } catch (error) {
    console.error("Error broadcasting transaction:", error);
  }
};

let measureTrxTime = async (address, info) => {
  console.time("trxTime");
  for (let i = 0; i < 10; i++) {
  
  }
  await broadcast(player2.address, client2, contractInfo, start_game);

  console.timeEnd("trxTime");
};

measureTrxTime(player2.address, contractInfo);


// execute(wallet.address, client1, contractInfo, start_game);