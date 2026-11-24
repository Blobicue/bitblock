fn main() {
    if let Err(err) = bitblock::testnet_1::testnet_proc::run_testnet_node_cli() {
        eprintln!("BOOT_HALT: {err}");
        std::process::exit(1);
    }
}
