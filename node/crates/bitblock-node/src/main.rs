//! Standalone CLI binary for the merged Bitblock L1 Bits testnet daemon.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "bitblock-node",
    about = "Bitblock L1 Bits Testnet Validator & Full Node (Testnet/Devnet only)"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Generate a reproducible local multi-validator shadow testnet with encrypted keystores.
    InitTestnet {
        #[arg(long, default_value = "node/.runtime/testnet")]
        dir: PathBuf,
        #[arg(long, default_value_t = 4)]
        validators: usize,
        #[arg(long, default_value_t = 19100)]
        base_p2p_port: u16,
        #[arg(long, default_value_t = 8645)]
        base_rpc_port: u16,
        #[arg(long, default_value_t = 1000)]
        block_ms: u64,
        #[arg(long)]
        epoch_blocks: Option<u64>,
        #[arg(long, default_value = "bitblock-testnet-local-pass")]
        keystore_password: String,
        /// Additional authorized grant relayer Ed25519 public key hex (repeatable).
        #[arg(long)]
        grant_relayer_pk: Vec<String>,
    },
    /// Run a Bitblock L1 validator or full node from its JSON config.
    Run {
        #[arg(long)]
        config: PathBuf,
        /// Path to passphrase file for `validator_keystore_path`.
        #[arg(long)]
        password_file: Option<PathBuf>,
        /// Allow plaintext seed fields inside `node.json` (dev/CI only; refused by default).
        #[arg(long, default_value_t = false)]
        unsafe_dev_keys: bool,
        /// Emit structured JSON tracing logs.
        #[arg(long, default_value_t = false)]
        log_json: bool,
    },
}

#[tokio::main]
async fn main() -> Result<(), String> {
    let cli = Cli::parse();
    match cli.cmd {
        Command::InitTestnet {
            dir,
            validators,
            base_p2p_port,
            base_rpc_port,
            block_ms,
            epoch_blocks,
            keystore_password,
            grant_relayer_pk,
        } => {
            let res = bitblock_node::init_testnet(
                dir,
                validators,
                base_p2p_port,
                base_rpc_port,
                block_ms,
                epoch_blocks,
                keystore_password,
                true,
                grant_relayer_pk,
            )?;
            println!(
                "Initialized Bitblock L1 shadow testnet (lab=true):\n  genesis: {}\n  genesis_id: {}\n  faucet_relayer_pk: {}\n  nodes: {}",
                res.genesis_path.display(),
                res.genesis_id_hex,
                res.faucet_relayer_pubkey_hex,
                res.node_configs.len()
            );
            Ok(())
        }
        Command::Run {
            config,
            password_file,
            unsafe_dev_keys,
            log_json,
        } => {
            if log_json {
                let _ = tracing_subscriber::fmt().json().try_init();
            } else {
                let _ = tracing_subscriber::fmt().try_init();
            }
            bitblock_node::run_node_with_shutdown(config, password_file, unsafe_dev_keys, None)
                .await
        }
    }
}
