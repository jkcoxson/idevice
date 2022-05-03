// jkcoxson

use tokio::{io::AsyncReadExt, net::TcpListener};

#[tokio::main]
async fn main() {
    // Create a TCP listener on port 0xf27e
    let listener = TcpListener::bind("0.0.0.0:62078").await.unwrap();

    // Listen for incoming connections
    loop {
        let (mut socket, _) = listener.accept().await.unwrap();

        tokio::spawn(async move {
            let mut buf = [0; 4096];
            let size = socket.read(&mut buf).await.unwrap();
            println!("{:?}", buf[..].to_vec());
            println!("{}", String::from_utf8_lossy(&buf[..size]));
        });
    }
}
