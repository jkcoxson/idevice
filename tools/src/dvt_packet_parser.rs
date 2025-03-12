// Jackson Coxson

use idevice::dvt::message::Message;

#[tokio::main]
async fn main() {
    let file = std::env::args().nth(1).expect("No file passed");
    let mut bytes = tokio::fs::File::open(file).await.unwrap();

    let message = Message::from_reader(&mut bytes).await.unwrap();
    println!("{message:#?}");

    println!("----- AUX -----");
    if let Some(aux) = message.aux {
        for v in aux.values {
            match v {
                idevice::dvt::message::AuxValue::Array(a) => {
                    match ns_keyed_archive::decode::from_bytes(&a) {
                        Ok(a) => {
                            println!("{a:#?}");
                        }
                        Err(_) => {
                            println!("{a:?}");
                        }
                    }
                }
                _ => {
                    println!("{v:?}");
                }
            }
        }
    }
}
