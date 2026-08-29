mod server;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    server::Server::default().run()?;
    Ok(())
}
