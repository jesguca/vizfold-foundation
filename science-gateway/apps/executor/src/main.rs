#[tokio::main]
async fn main() {
    let mut argv = std::env::args().skip(1);
    match argv.next().as_deref() {
        Some("run") => executor::adapters::cli::main(argv).await,
        None | Some("serve") => executor::adapters::rest::serve().await,
        Some(command) => {
            eprintln!(
                "unknown command '{command}'\n\n{}",
                executor::adapters::cli::USAGE
            );
            std::process::exit(2);
        }
    }
}
