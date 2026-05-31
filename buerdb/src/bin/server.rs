use mini_redis::protocol::server;
use std::error::Error;

#[cfg(not(target_env = "msvc"))]
use tikv_jemallocator::Jemalloc;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("Server initialising");
    server::handle_requests("127.0.0.1:6379").await?;
    Ok(())
}
