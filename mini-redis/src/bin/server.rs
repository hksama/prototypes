use mini_redis::protocol::server;
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("Server initialising");
    server::handle_requests("127.0.0.1:8080").await?;
    Ok(())
}
