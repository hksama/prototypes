// use crate::protocol::server::Commands;
use crate::protocol::resp::Command;
use std::{
    error::Error,
    io::{stdout, Write},
    net::SocketAddr,
};
use tokio::{io::AsyncWriteExt, net::TcpStream};

pub async fn make_requests(addr: SocketAddr) -> Result<(), Box<dyn Error>> {
    let mut stream = TcpStream::connect("127.0.0.1:6379").await?;

    // Not cancellation safe.
    stream.write_all(b"hello").await?;
    loop {
        stream.readable().await?;
        let mut buf = Vec::with_capacity(4096);

        // Try to read data, this may still fail with `WouldBlock`
        // if the readiness event is a false positive.
        match stream.try_read_buf(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                println!("read {} bytes", n);
                let text = str::from_utf8(&buf[..n]).unwrap();
                println!("{}", text);
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                continue;
            }
            Err(e) => {
                return Err(e.into());
            }
        }
    }

    Ok(())
}

pub fn event_loop() {
    // todo!("Implement Event Loop for client")
    loop {
        let mut input = String::new();

        std::io::stdin()
            .read_line(&mut input)
            .expect("Failed to read line");

        let cmd_args: Vec<&str> = input.trim().split_whitespace().collect();

        if cmd_args.is_empty() {
            print!("Invalid input format: {}", input);
            print!("127.0.0.1:8080>");
            stdout().flush().unwrap();
            continue;
        }
        match cmd_args[0] {
            "GET" | "get" => {
                if cmd_args.len() < 2 {
                    println!("GET requires key");
                    continue;
                }
                handle_get_client(Command::GET(cmd_args[1].to_owned()), Some(&cmd_args[2..]))
            }
            "SET" | "set" => {
                if cmd_args.len() < 3 {
                    println!("SET requires key and value");
                    continue;
                }
                handle_set_client(
                    Command::SET(cmd_args[1].to_owned(), cmd_args[2].to_owned()),
                    Some(&cmd_args[3..]),
                )
            }
            "DEL" | "del" => {
                if cmd_args.len() < 2 {
                    println!("DEL requires key");
                    continue;
                }
                handle_del_client(Command::DEL(cmd_args[1].to_owned()), Some(&cmd_args[2..]))
            }
            "PING" | "ping" => {
                handle_ping_client();
            }
            _ => {
                println!("Unknown command: {}", cmd_args[0]);
            }
        }
        print!("127.0.0.1:8080>");
        stdout().flush().unwrap();
    }
}

fn handle_get_client(cmd: Command, flags: Option<&[&str]>) {
    // check what flags can be passed
}

fn handle_set_client(cmd_args: Command, flags: Option<&[&str]>) {
    // check what flags can be passed
}

fn handle_del_client(cmd_args: Command, flags: Option<&[&str]>) {
    // check what flags can be passed
}

fn handle_ping_client() {}

// fn encode_message(cmd: Commands) -> String {}
