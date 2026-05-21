use bytes::BytesMut;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, ReadBuf},
    net::TcpStream, time::timeout};
use std::io::{stdout,Write};
pub async fn process_requests(mut socket: TcpStream){
    let mut buf = BytesMut::with_capacity(4096);
    // let mut buf = [0; 1024];
            // let response = String::from("Response Message").as_bytes();
            // Returns number of bytes read; 0 means connection closed
    
    let max_run_time = std::time::Duration::from_secs(5);
    loop {
            match socket.read_buf(&mut buf).await {
                Ok(0) => return,
                Ok(n) => {
                    println!("Read {} bytes: {:?}", n, &buf[..n]);
                    let text = str::from_utf8(&buf[..n]).unwrap();
                    println!("{}", text);
                    socket
                    .write_all(&String::from("Response Message").as_bytes()[..])
                    .await
                    .unwrap();
                socket.shutdown().await.unwrap();
                stdout().flush().unwrap();
                print!("127.0.0.1:8080>");
            }
            Err(e) => {
                eprintln!("Failed to read: {}", e);
                return;
            }
        }
    }
}