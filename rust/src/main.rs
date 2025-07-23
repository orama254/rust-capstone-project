#![allow(unused)]
use bitcoin::hex::DisplayHex;
use bitcoincore_rpc::bitcoin::{Address, Amount, Network};
use bitcoincore_rpc::{Auth, Client, RpcApi};
use serde::Deserialize;
use serde_json::json;
use std::fs::File;
use std::io::Write;
use std::path::Path;

// Node access params
const RPC_URL: &str = "http://127.0.0.1:18443"; // Default regtest RPC port
const RPC_USER: &str = "alice";
const RPC_PASS: &str = "password";

const COINBASE_MATURITY: u64 = 100;
const MINER_WALLET_NAME: &str = "Miner";
const TRADER_WALLET_NAME: &str = "Trader";
const MINER_ADDRESS_LABEL: &str = "Mining Reward";
const TRADER_ADDRESS_LABEL: &str = "Received";
const TRANSACTION_AMOUNT_TO_SEND: f64 = 20.0;

fn verify_wallet(rpc: &Client, wallet_name: &str) -> bitcoincore_rpc::Result<Client> {
    let wallet_names_in_dir = rpc.list_wallet_dir()?;
    let wallet_exists = wallet_names_in_dir.iter().any(|w| w == wallet_name);

    if !wallet_exists {
        println!("Creating Wallet: '{wallet_name}'...");
        rpc.create_wallet(wallet_name, None, None, None, None)?;
    } else {
        println!("Wallet '{wallet_name}' already exists in directory");
    }

    let loaded_wallet_names = rpc.list_wallets()?;
    if !loaded_wallet_names.contains(&wallet_name.to_string()) {
        println!("Loading Wallet: '{wallet_name}'...");
        rpc.load_wallet(wallet_name)?;
    } else {
        println!("Wallet '{wallet_name}' is already loaded");
    }

    let wallet_url = format!("{RPC_URL}/wallet/{wallet_name}");
    let wallet_client = Client::new(
        &wallet_url,
        Auth::UserPass(RPC_USER.to_owned(), RPC_PASS.to_owned()),
    )?;

    Ok(wallet_client)
}

fn main() -> bitcoincore_rpc::Result<()> {
    // Connect to Bitcoin Core RPC
    let rpc = Client::new(
        RPC_URL,
        Auth::UserPass(RPC_USER.to_owned(), RPC_PASS.to_owned()),
    )?;

    // Get blockchain info
    let blockchain_info = rpc.get_blockchain_info()?;
    println!("Blockchain Info: {blockchain_info:#?}");

    let miner_rpc = verify_wallet(&rpc, MINER_WALLET_NAME)?;

    let trader_rpc = verify_wallet(&rpc, TRADER_WALLET_NAME)?;

    let miner_address = miner_rpc
        .get_new_address(Some(MINER_ADDRESS_LABEL), None)?
        .assume_checked();
    println!("Miner Address: {miner_address}");

    println!(
        "Mining {} blocks to mature coinbase transaction (100 blocks maturity + 1 block for the initial reward)...",
        COINBASE_MATURITY + 1
    );

    rpc.generate_to_address(COINBASE_MATURITY + 1, &miner_address)?;

    let miner_balance = miner_rpc.get_balance(None, None)?;
    println!(
        "Miner wallet spendable balance: {:.8} BTC (after {} total blocks)",
        miner_balance.to_btc(),
        rpc.get_block_count()?
    );

    let trader_address = trader_rpc
        .get_new_address(Some(TRADER_ADDRESS_LABEL), None)?
        .assume_checked();
    println!("Trader Address: {trader_address}");

    let amount_to_send = Amount::from_btc(TRANSACTION_AMOUNT_TO_SEND)?;

    let transaction_id = miner_rpc.send_to_address(
        &trader_address,
        amount_to_send,
        None,
        None,
        None,
        None,
        None,
        None,
    )?;
    println!(
        "Sent {TRANSACTION_AMOUNT_TO_SEND:.8} BTC from Miner to Trader. Transaction ID: {transaction_id}",
    );

    let mempool_entry = rpc.get_mempool_entry(&transaction_id)?;
    println!("Mempool entry for transaction {transaction_id}: {mempool_entry:#?}",);

    println!("Mining 1 block to confirm the transaction...");

    let confirmation_block_hashes = rpc.generate_to_address(1, &miner_address)?;
    let confirmation_block_hash = confirmation_block_hashes[0];
    println!("Transaction {transaction_id} confirmed in block: {confirmation_block_hash}",);

    let tx_details = miner_rpc.get_transaction(&transaction_id, None)?;

    let raw_tx = miner_rpc.get_raw_transaction(&transaction_id, None)?;

    let block_height = tx_details
        .info
        .blockheight
        .expect("Confirmed transaction must have a block height.");
    let block_hash = tx_details
        .info
        .blockhash
        .expect("Confirmed transaction must have a block hash.");

    let first_input = &raw_tx.input[0];
    let prev_txid = first_input.previous_output.txid;
    let prev_vout_index = first_input.previous_output.vout;

    let prev_raw_tx = rpc.get_raw_transaction(&prev_txid, None)?;

    let prev_output = &prev_raw_tx.output[prev_vout_index as usize];

    let miner_input_address = Address::from_script(&prev_output.script_pubkey, Network::Regtest)
        .expect("Failed to decode miner's input address from script")
        .to_string();

    let miner_input_amount = prev_output.value.to_btc();

    let mut trader_output_address: String = String::new();
    let mut trader_output_amount: f64 = 0.0;
    let mut miner_change_address: String = String::new();
    let mut miner_change_amount: f64 = 0.0;

    for output in &raw_tx.output {
        let output_address = Address::from_script(&output.script_pubkey, Network::Regtest)
            .expect("Failed to decode output address from script");
        let output_amount_btc = output.value.to_btc();

        if output_address == trader_address {
            trader_output_address = output_address.to_string();
            trader_output_amount = output_amount_btc;
        } else {
            miner_change_address = output_address.to_string();
            miner_change_amount = output_amount_btc;
        }
    }

    let transaction_fees = tx_details.fee.unwrap().to_btc().abs();

    let out_path = Path::new("../out.txt");
    let mut output_file = File::create(out_path)?;

    writeln!(output_file, "{transaction_id}")?;
    writeln!(output_file, "{miner_input_address}")?;
    writeln!(output_file, "{miner_input_amount:.8}")?;
    writeln!(output_file, "{trader_output_address}")?;
    writeln!(output_file, "{trader_output_amount:.8}")?;
    writeln!(output_file, "{miner_change_address}")?;
    writeln!(output_file, "{miner_change_amount:.8}")?;
    writeln!(output_file, "{transaction_fees:.8}")?;
    writeln!(output_file, "{block_height}")?;
    writeln!(output_file, "{block_hash}")?;

    println!("\nTransaction details written to ../out.txt successfully!");
    println!("Program completed successfully!");

    Ok(())
}
