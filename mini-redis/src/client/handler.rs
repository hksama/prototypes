use std::{error::Error, net::SocketAddr};
use tokio::{io::AsyncWriteExt, net::TcpStream};

pub async fn make_requests(addr: SocketAddr) -> Result<(), Box<dyn Error>> {
    let mut stream = TcpStream::connect(addr).await?;

    // Not cancellation safe.
    stream.write_all(b"hello").await?;

    Ok(())
}

pub fn event_loop() {
    todo!("Implement Event Loop for client")
}
