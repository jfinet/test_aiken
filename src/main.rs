use std::env;

use blockfrost::{BlockFrostApi, BlockFrostSettings};
use cardano_serialization_lib::{
    address::Address,
    crypto::{PrivateKey, TransactionHash, Vkey, Vkeywitness, Vkeywitnesses},
    output_builder::TransactionOutputBuilder,
    plutus::{
        ConstrPlutusData, Costmdls, ExUnits, Language, PlutusData, PlutusList, PlutusScript,
        PlutusScripts, Redeemer, RedeemerTag, Redeemers,
    },
    tx_builder::{tx_inputs_builder::ScriptWitnessType, TransactionBuilder},
    tx_builder_constants::TxBuilderConstants,
    utils::{hash_script_data, hash_transaction, to_bignum, BigNum},
    Ed25519KeyHashes, Transaction, TransactionBody, TransactionInput, TransactionInputs,
    TransactionOutputs, TransactionWitnessSet,
};
use hex::encode;
use minicbor::{Decoder, Encode, Encoder};

#[tokio::main]
async fn main() {
    //send_utxo_to_script().await;
    get_utxo_from_script().await;
}

async fn get_utxo_from_script() {
    // INIT
    let blockfrost_api = prepare_blockfrost_testnet_api();
    let payer_address =
        Address::from_bech32("addr_test1vpslrka28q7net6pv8u2yh37kyjddts72c22f4dlx5achaql8phjv")
            .unwrap();
    let fee = to_bignum(6_000_000);
    let prv_key = PrivateKey::from_bech32("ed25519e_sk1mr6ts7wu8mf8swmqyyp68vwnytzvm74wcpt53g6kvcvztx4244q4mkrzglque0q3mkelahwqz6pg3enp39rq80mrefq8zj80u3ct06ggglm8w").unwrap();

    // SCRIPT INPUT
    let tx_hash = TransactionHash::from_hex(
        "7ddaf99045163d5df59d28ae9b063df01286306dcd938a727b05732eb11aadbc", //8 Ada in it
    )
    .unwrap(); //todo fill manually
    let tx_index = 0;
    let mut inputs = TransactionInputs::new();
    let script_input = TransactionInput::new(&tx_hash, tx_index);
    inputs.add(&script_input);

    // COLLATERAL INPUT
    let tx_hash = TransactionHash::from_hex(
        "6d6f22ffa1f78b82efca1d01d4fbb9a7e5322b06be1eff070d0a5fb6a4f2007b", //10 ada in it
    )
    .unwrap(); //todo fill manually
    let tx_index = 0;
    let mut collateral_inputs = TransactionInputs::new();
    let collateral_input = TransactionInput::new(&tx_hash, tx_index);
    collateral_inputs.add(&collateral_input);

    // OUTPUT
    let mut outputs = TransactionOutputs::new();
    let output_builder = TransactionOutputBuilder::new().with_address(&payer_address);
    let payer_output = output_builder
        .next()
        .unwrap()
        .with_coin(&to_bignum(2_000_000))
        .build()
        .unwrap();
    outputs.add(&payer_output);

    let mut tx_body = TransactionBody::new_tx_body(&inputs, &outputs, &fee);

    //SET COLLATERAL
    tx_body.set_collateral(&collateral_inputs);

    // SCRIPT
    // from aiken payment_script.json
    //let cbor_hex = "58bb58b90100002225333573464646464a666ae6800840045281919198009bac330043005330043005006480012010375c66008600a01090001800800911192999aab9f00114a026464a666ae68cdc78010020a511333006006001004357440046eb8d5d080080119b97323001375c66004600600a900011b9900149010d48656c6c6f2c20576f726c64210022323330010014800000c888cccd5cd19b870040025742466600800866e0000d20023574400200246aae78dd50008a4c2d";
    // from aiken script.cbor
    let cbor_hex = "58b90100002225333573464646464a666ae6800840045281919198009bac330043005330043005006480012010375c66008600a01090001800800911192999aab9f00114a026464a666ae68cdc78010020a511333006006001004357440046eb8d5d080080119b97323001375c66004600600a900011b9900149010d48656c6c6f2c20576f726c64210022323330010014800000c888cccd5cd19b870040025742466600800866e0000d20023574400200246aae78dd50008a4c2d";
    let cbor = hex::decode(cbor_hex).unwrap();

    let mut plutus_scripts = PlutusScripts::new();
    let plutus_script = PlutusScript::new_v2(cbor);

    let script_hash = plutus_script.hash();
    println!("script_hash: {}", script_hash);
    plutus_scripts.add(&plutus_script);

    let mut witness_set = TransactionWitnessSet::new();
    witness_set.set_plutus_scripts(&plutus_scripts);

    // REDEEMER
    let hello_world = "Hello, World!".as_bytes().to_vec(); //hex::encode("Hello, World!").as_bytes().to_vec(); //
    let hello_world_plutus_data = PlutusData::new_bytes(hello_world);
    let mut plutus_list = PlutusList::new();
    plutus_list.add(&hello_world_plutus_data);
    let constr_plutus_data =
        PlutusData::new_constr_plutus_data(&ConstrPlutusData::new(&to_bignum(0), &plutus_list));

    let mem = to_bignum(11_250_000);
    let steps = to_bignum(10_000_000_000);
    let redeemer = Redeemer::new(
        &RedeemerTag::new_spend(),
        &BigNum::zero(),
        &constr_plutus_data, //constr_plutus_data
        &ExUnits::new(&mem, &steps),
    );
    let mut redeemers = Redeemers::new();
    redeemers.add(&redeemer);
    witness_set.set_redeemers(&redeemers);

    // SCRIPT_HASH
    let mut cost_models = Costmdls::new();
    let language = Language::new_plutus_v2();
    let cost_model = TxBuilderConstants::plutus_vasil_cost_models()
        .get(&language)
        .unwrap();
    cost_models.insert(&language, &cost_model);
    let script_data_hash = hash_script_data(&redeemers, &cost_models, None); //Some(plutus_list)); //Some(plutus_data) //None
    tx_body.set_script_data_hash(&script_data_hash);

    // ADD REQUIRED SIGNER
    let mut signers = Ed25519KeyHashes::new();
    signers.add(&prv_key.to_public().hash());
    tx_body.set_required_signers(&signers);

    // SIGNATURE
    let mut vkey_witnesses = Vkeywitnesses::new();
    let signature = prv_key.sign(&hash_transaction(&tx_body).to_bytes());
    let v_key = Vkey::new(&prv_key.to_public());
    vkey_witnesses.add(&Vkeywitness::new(&v_key, &signature));
    witness_set.set_vkeys(&vkey_witnesses);

    // SEND TX
    let tx = Transaction::new(&tx_body, &witness_set, None);
    let blockfrost_response = blockfrost_api
        .transactions_submit(tx.to_bytes())
        .await
        .unwrap();
    println!("tx_hash: '{}'", blockfrost_response);
}

async fn send_utxo_to_script() {
    // INIT
    let blockfrost_api = prepare_blockfrost_testnet_api();
    let fee = to_bignum(2_000_000);
    let script_address =
        Address::from_bech32("addr_test1wr534h6y6yns9xtaaau0xnpq3kqjtvlazt533ffgquvs7sg9zx9yh")
            .unwrap();
    let payer_address =
        Address::from_bech32("addr_test1vpslrka28q7net6pv8u2yh37kyjddts72c22f4dlx5achaql8phjv")
            .unwrap();
    let prv_key = PrivateKey::from_bech32("ed25519e_sk1mr6ts7wu8mf8swmqyyp68vwnytzvm74wcpt53g6kvcvztx4244q4mkrzglque0q3mkelahwqz6pg3enp39rq80mrefq8zj80u3ct06ggglm8w").unwrap();

    // PREPARE PLUTUS DATA
    let pkh = prv_key.to_public().hash();
    let pkh_plutus_data = PlutusData::new_bytes(pkh.to_bytes());
    let mut plutus_list = PlutusList::new();
    plutus_list.add(&pkh_plutus_data);
    let constr_plutus_data =
        PlutusData::new_constr_plutus_data(&ConstrPlutusData::new(&to_bignum(0), &plutus_list));

    // INPUT
    let tx_hash = TransactionHash::from_hex(
        "797b88ac729a01ce10f1dabc7c1fcbc30a52a7b07b45fc405aeb2c00ad1d5bbc",
    )
    .unwrap(); //todo fill manually
    let tx_index = 0;
    let mut inputs = TransactionInputs::new();
    let input = TransactionInput::new(&tx_hash, tx_index);
    inputs.add(&input);

    // SCRIPT OUTPUT
    let mut outputs = TransactionOutputs::new();
    let output_builder = TransactionOutputBuilder::new().with_address(&script_address);
    let output_builder = output_builder.with_plutus_data(&constr_plutus_data);
    let script_output = output_builder
        .next()
        .unwrap()
        .with_coin(&to_bignum(8_000_000))
        .build()
        .unwrap();
    outputs.add(&script_output);

    let tx_body = TransactionBody::new_tx_body(&inputs, &outputs, &fee);

    // ADD SIGNATURE
    let mut witness_set = TransactionWitnessSet::new();
    let mut vkey_witnesses = Vkeywitnesses::new();
    let signature = prv_key.sign(&hash_transaction(&tx_body).to_bytes());
    let v_key = Vkey::new(&prv_key.to_public());
    vkey_witnesses.add(&Vkeywitness::new(&v_key, &signature));
    witness_set.set_vkeys(&vkey_witnesses);

    // SEND TX
    let tx = Transaction::new(&tx_body, &witness_set, None);
    let blockfrost_response = blockfrost_api
        .transactions_submit(tx.to_bytes())
        .await
        .unwrap();
    println!("tx_hash: '{}'", blockfrost_response);
}

pub fn prepare_blockfrost_testnet_api() -> BlockFrostApi {
    let blockfrost_project_id = env::var("BLOCKFROST_PROJECT_ID")
        .unwrap_or("preprodcd7lIdgNxfOMKa61b2RddfvaSV3eQoWe".to_string());
    let mut cardano_network = BlockFrostSettings::new().use_testnet(); //todo check if need another lambda for different env
                                                                       // may need to check the value of .getNetworkId() from the connected wallet?
    cardano_network.network_address = "https://cardano-preprod.blockfrost.io/api/v0".to_string();
    BlockFrostApi::new(blockfrost_project_id, cardano_network)
}
