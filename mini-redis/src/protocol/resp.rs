use crate::protocol::error::RespError;
// use anyhow::bail;
use bytes::{BufMut, Bytes, BytesMut};

pub enum Command {
    GET(String),
    SET(String, String),
    DEL(String),
    PING,
}

pub enum RespFrames {
    SimpleString(Bytes),
    Integer(i64),
    /// Length of bulk string followed by Bytes of that length.
    BulkString(usize, Bytes),
    Array(Vec<Bytes>),
    Error,
    Null,
}

pub fn match_datatype_to_cmd(cmd_type: u8) {
    match cmd_type {
        b'+' => {
            handle_simple_strings();
        }
        b':' => {
            handle_integers();
        }
        b'$' => {
            handle_bulk_strings();
        }
        b'*' => {
            handle_arrays();
        }
        b'-' => {
            handle_errors();
        }
        _ => {
            todo!("Implement Handling of Error")
        }
    }
}

fn handle_integers() {
    todo!("Implement Handling of Integers");
}

fn handle_simple_strings() {
    todo!("Implement Handling of Simple Strings");
}

fn handle_bulk_strings() {
    todo!("Implement Handling of Bulks Strings");
}

fn handle_arrays() {
    todo!("Implement Handling of Arrays");
}

fn handle_errors() {
    todo!("Implement Handling of Errors");
}

pub trait Encode {
    fn convert_string_to_cmd(cli_cmd: String) -> Result<(), RespError> {
        // for c in cli_cmd.chars() {
        //     match c {
        //         ' '
        //     }
        // }

        let cmd_split: Vec<&str> = cli_cmd.trim().split_whitespace().collect();
        if cmd_split.is_empty() {
            // bail!("Missing Input".into());
            eprintln!("Not enough input ");
            return Err(RespError::InvalidArgLength);
        }
        match cmd_split[0] {
            "GET" | "get" => {
                if cmd_split.len() < 2 {
                    println!("GET requires key");
                    return Err(RespError::InvalidArgLength);
                    // continue;
                }
                Self::handle_get_client(
                    Command::GET(cmd_split[1].to_owned()),
                    Some(&cmd_split[2..]),
                )?;
                Ok(())
            }
            "SET" | "set" => {
                if cmd_split.len() < 3 {
                    println!("SET requires key and value");
                    return Err(RespError::InvalidArgLength);
                    // continue;
                }
                Self::handle_set_client(
                    Command::SET(cmd_split[1].to_owned(), cmd_split[2].to_owned()),
                    Some(&cmd_split[3..]),
                );
                Ok(())
            }
            "DEL" | "del" => {
                if cmd_split.len() < 2 {
                    println!("DEL requires key");
                    return Err(RespError::InvalidArgLength);
                    // continue;
                }
                Self::handle_del_client(
                    Command::DEL(cmd_split[1].to_owned()),
                    Some(&cmd_split[2..]),
                );
                Ok(())
            }
            "PING" | "ping" => {
                Self::handle_ping_client();
                Ok(())
            }
            _ => {
                println!("Unknown command: {}", cmd_split[0]);
                return Err(RespError::UnknownCommand);
            }
        }
        // todo!("Implement Handling of Client Commands")
    }
    //convert pub to private
    fn convert_cmd_to_resp_frames<'a>(cmd: Command) -> &'a [RespFrames] {
        match cmd {
            Command::GET(key) => {
                return &[
                    RespFrames::BulkString("get".to_string().len(), Bytes::from("get")),
                    RespFrames::BulkString(key.len(), Bytes::from(key)),
                ];
            }
            Command::SET(key, value) => {
                return &[
                    RespFrames::BulkString("set".len(), Bytes::from("set")),
                    RespFrames::BulkString(key.len(), Bytes::from(key)),
                    RespFrames::BulkString(value.len(), Bytes::from(value)),
                ];
            }
            Command::DEL(key) => {
                return &[RespFrames::BulkString(key.len(), Bytes::from(key))];
            }
            Command::PING => {
                return &[RespFrames::BulkString("ping".len(), Bytes::from("ping"))];
            }
            _ => {
                // vec![RespFrames::Null];
                return &[RespFrames::Error];
            }
        }
        todo!("Implement Handling of RESP Frames")
    }

    fn convert_resp_frames_to_bytes<'a>(resp: &'a [RespFrames]) -> BytesMut {
        // BytesMut::new()
        for resp_frame in resp {
            match resp_frame {
                RespFrames::BulkString(len, buf) => {
                    // Reserve capacity for "$len\r\n" + string bytes + "\r\n"
                    let mut buffer = BytesMut::with_capacity(*len + 6);
                    buffer.put(
                        "$".to_owned()
                            + &len.to_string()
                            + "\r\n".to_owned()
                            + buf.to_string()
                            + "\r\n".to_owned(),
                    );
                }
                _ => {
                    todo!("Implement Handling of Other RESP Frames")
                }
            }
        }
        todo!("Implement Handling of Bytes")
    }
    fn handle_get_client(cmd: Command, flags: Option<&[&str]>) -> Result<(), RespError> {
        // check what flags can be passed

        //check if GET only
        if !matches!(cmd, Command::GET(_)) {
            return Err(RespError::UnexpectedCommand);
        } else {
            Self::convert_cmd_to_resp_frames(cmd);
            Ok(())
        }
    }

    fn handle_set_client(cmd_args: Command, flags: Option<&[&str]>) {
        // check what flags can be passed
    }

    fn handle_del_client(cmd_args: Command, flags: Option<&[&str]>) {
        // check what flags can be passed
    }

    fn handle_ping_client() {}
}

// fn conver(cmd: &Command) {
//     // match cmd {
//     //     Command::GET(_key) => ,
//     //     Command::SET(_key, _value) => Ok(()),
//     //     Command::DEL(_key) => Ok(()),
//     //     Command::PING => Ok(()),
//     // }
//     todo!("Complete")
// }
pub trait Decode {}
