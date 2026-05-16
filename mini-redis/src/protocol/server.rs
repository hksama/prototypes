use bytes::{BufMut, Bytes, BytesMut};
use std::{error::Error, net::SocketAddr};
use tokio::{
    io::AsyncReadExt,
    net::{TcpListener, TcpStream},
};

use crate::client::handler::make_requests;

async fn process_socket(mut socket: TcpStream, addr: SocketAddr) -> Result<(), Box<dyn Error>> {
    println!("{:?}", socket);

    // loop {
    //     make_requests(addr).await?;
    // }
    tokio::spawn(async move {
        // let mut buf = BytesMut::with_capacity(1024);
        let mut buf = [0; 1024];
        loop {
            // Returns number of bytes read; 0 means connection closed
            match socket.read(&mut buf).await {
                Ok(0) => return,
                Ok(n) => {
                    println!("Read {} bytes: {:?}", n, &buf[..n]);
                    let text = str::from_utf8(&buf[..n - 1]).unwrap();
                    println!("{}", text);
                }
                Err(e) => {
                    eprintln!("Failed to read: {}", e);
                    return;
                }
            }
        }
    });
    make_requests(addr).await?;

    Ok(())
}

pub async fn handle_requests(sock_addr: &str) -> Result<(), Box<dyn Error>> {
    let listener = TcpListener::bind(sock_addr).await?;

    loop {
        let (mut socket, addr) = listener.accept().await?;
        process_socket(socket, addr).await?;
    }
}
