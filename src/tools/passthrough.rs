// Sniffs packets going from computer to iOS device

use colored::Colorize;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

const TARGET_ADDR: &str = "192.168.1.14:62078";

#[tokio::main]
async fn main() {
    // Listen on localhost:62078
    let listener = TcpListener::bind("localhost:62078").await.unwrap();

    loop {
        let (mut socket, _) = listener.accept().await.unwrap();

        tokio::spawn(async move {
            // Connect to the target
            let mut target = TcpStream::connect(TARGET_ADDR).await.unwrap();
            println!("Connected to {}", TARGET_ADDR);

            // Copy data between the two sockets
            let mut buf1 = [0; 4096];
            let mut buf2 = [0; 4096];

            loop {
                tokio::select! {
                    size = socket.read(&mut buf1) => {
                        if size.is_err() {
                            println!("{}", "Error reading from socket".red());
                            return;
                        }
                        let buf1 = &buf1[..size.unwrap()];
                        println!("{}", String::from_utf8_lossy(buf1).green());
                        target.write_all(buf1).await.unwrap();
                    },
                    size = target.read(&mut buf2) => {
                        if size.is_err() {
                            println!("{}", "Error reading from target".red());
                            return;
                        }
                        let buf2 = &buf2[..size.unwrap()];
                        println!("{}", String::from_utf8_lossy(buf2).blue());
                        socket.write_all(buf2).await.unwrap();
                    }
                }
            }
        });
    }
}
