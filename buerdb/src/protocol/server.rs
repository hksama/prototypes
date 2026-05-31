use crate::protocol::network::process_requests;
use std::{error::Error, net::SocketAddr};
use tokio::net::{TcpListener, TcpStream};

async fn process_socket(socket: TcpStream, addr: SocketAddr) -> Result<(), Box<dyn Error>> {
    println!("{:?}", socket);

    // loop {
    //     make_requests(addr).await?;
    // }
    tokio::spawn(async move {
        process_requests(socket).await;
    });

    Ok(())
}

pub async fn handle_requests(sock_addr: &str) -> Result<(), Box<dyn Error>> {
    let listener = TcpListener::bind(sock_addr).await?;
    println!("Server running on {} ", sock_addr);
    loop {
        let (mut socket, addr) = listener.accept().await?;
        println!("New connection from {}", addr);
        process_socket(socket, addr).await?;
    }
}
