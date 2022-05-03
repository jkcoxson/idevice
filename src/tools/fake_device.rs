// jkcoxson

use plist::Value;
use serde::Serialize;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

#[tokio::main]
async fn main() {
    // Lockdownd listener
    let listener = TcpListener::bind("0.0.0.0:62078").await.unwrap();

    // Listen for incoming connections to lockdownd
    loop {
        let (socket, _) = listener.accept().await.unwrap();

        tokio::spawn(async move {
            handle_lockdown_connection(socket).await;
        });
    }
}

async fn handle_lockdown_connection(mut socket: TcpStream) {
    loop {
        let mut buf = [0; 4096];
        let size = match socket.read(&mut buf).await {
            Ok(s) => s,
            Err(_) => break,
        };
        println!("{:?}", buf[..size].to_vec());
        println!("{}", String::from_utf8_lossy(&buf[..size]));

        // Parse the request as a plist
        let request: Value = plist::from_bytes(&buf[4..size]).unwrap();
        println!("{:?}", request);
        let request = request.into_dictionary().unwrap();
        match request.get("Request").unwrap().as_string().unwrap() {
            "QueryType" => {
                #[derive(Serialize)]
                #[serde(rename_all = "PascalCase")]
                struct Response {
                    request: String,
                    type_: String,
                }
                let res = Response {
                    request: "QueryType".to_string(),
                    type_: "com.apple.mobile.lockdown".to_string(),
                };
                // Serialize the query to a plist
                let mut to_send = Vec::new();
                plist::to_writer_xml(&mut to_send, &res).unwrap();

                // Get the size of the packet and append it to the front
                let size = to_send.len() as u32;
                let mut size = size.to_be_bytes().to_vec();
                size.extend_from_slice(&to_send);

                // Send the packet to the host
                socket.write_all(&size).await.unwrap();
            }
            _ => {
                println!("Unknown request");
                break;
            }
        }
    }
    println!("Breaking connection\n");
}
