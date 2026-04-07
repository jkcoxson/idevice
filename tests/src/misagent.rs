// Jackson Coxson

use crate::run_test;
use idevice::{IdeviceService, provider::IdeviceProvider, services::misagent::MisagentClient};

pub async fn run_tests(provider: &dyn IdeviceProvider, success: &mut u32, failure: &mut u32) {
    run_test!("misagent: connect", success, failure, async {
        MisagentClient::connect(provider).await.map(|_| ())
    });

    let mut client = match MisagentClient::connect(provider).await {
        Ok(c) => c,
        Err(e) => {
            println!("  misagent: cannot connect ({e}), skipping remaining tests");
            *failure += 1;
            return;
        }
    };

    run_test!("misagent: copy_all profiles", success, failure, async {
        let profiles = client.copy_all().await?;
        println!("({} profiles)", profiles.len());
        Ok::<(), idevice::IdeviceError>(())
    });
}
