use mini_redis::client::handler::{event_loop, make_requests};
use std::{
    error::Error,
    io::{stdout, Write},
    net::{IpAddr, Ipv4Addr, SocketAddr},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    print!("127.0.0.1:8080>");
    stdout().flush()?;
    let socket_addr: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
    make_requests(socket_addr).await?;
    event_loop();

    Ok(())
}
