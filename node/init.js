import { SecretNetworkClient, Wallet } from "secretjs";
import * as fs from "fs";
// import dotenv from "dotenv";
// dotenv.config();

// const wallet = new Wallet(process.env.MNEMONIC);
const wallet = new Wallet("pigeon desk hammer sleep only mistake stool december offer patrol once vacant");
const contract_wasm = fs.readFileSync("../optimized-wasm/poker_cards_distributor.wasm.gz");

const secretjs = new SecretNetworkClient({
  chainId: "pulsar-3",
  url: "https://pulsar.lcd.secretnodes.com",
  wallet: wallet,
  walletAddress: wallet.address,
});

// Declare global variables for codeId and contractCodeHash
let codeId;
let contractCodeHash;

let contractInfo = {
  contractAddress: "",
  contractCodeHash: "",
}
let upload_contract = async () => {
  let tx = await secretjs.tx.compute.storeCode(
    {
      sender: wallet.address,
      wasm_byte_code: contract_wasm,
      source: "",
      builder: "",
    },
    {
      gasLimit: 1_500_000,
    }
  );

  codeId = Number(
    tx.arrayLog.find((log) => log.type === "message" && log.key === "code_id")
      .value
  );
  console.log("codeId: ", codeId);
  console.log(tx);
  contractCodeHash = (
    await secretjs.query.compute.codeHashByCodeId({ code_id: codeId })
  ).code_hash;
  console.log(`Contract hash: ${contractCodeHash}`);
  contractInfo.contractCodeHash = contractCodeHash;
};

let instantiate_contract = async () => {
  if (!codeId || !contractCodeHash) {
    throw new Error("codeId or contractCodeHash is not set.");
  }

  const initMsg = {};
  
  let tx = await secretjs.tx.compute.instantiateContract(
    {
      code_id: codeId,
      sender: wallet.address,
      code_hash: contractCodeHash,
      init_msg: initMsg,
      label: "poker_test" + Math.ceil(Math.random() * 10000),
    },
    {
      gasLimit: 50_000,
    }
  );
  console.log(tx);
  //Find the contract_address in the logs
  const contractAddress = tx.arrayLog.find(
    (log) => log.type === "message" && log.key === "contract_address"
  ).value;

  console.log("Contract address: ", contractAddress);
  contractInfo.contractAddress = contractAddress;
};

upload_contract()
  .then(() => {
    instantiate_contract().then(() => {
      fs.writeFileSync("contractInfo.json", JSON.stringify(contractInfo, null, 2));
    });
  })
  .catch((error) => {
    console.error("Error:", error);
  });

