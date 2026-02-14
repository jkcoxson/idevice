// Jackson Coxson

use idevice::{dvt::message::Message, provider::IdeviceProvider};
use jkcli::{CollectedArguments, JkArgument, JkCommand};

pub fn register() -> JkCommand {
    JkCommand::new()
        .help("Parse a DVT packet from a file")
        .with_argument(
            JkArgument::new()
                .required(true)
                .with_help("Path the the packet file"),
        )
}

pub async fn main(arguments: &CollectedArguments, _provider: Box<dyn IdeviceProvider>) {
    let mut arguments = arguments.clone();

    let file: String = arguments.next_argument().expect("No file passed");
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
