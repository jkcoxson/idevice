// Jackson Coxson

use crate::run_test;
use idevice::{
    IdeviceService, provider::IdeviceProvider,
    services::springboardservices::SpringBoardServicesClient,
};

pub async fn run_tests(provider: &dyn IdeviceProvider, success: &mut u32, failure: &mut u32) {
    run_test!("springboard: connect", success, failure, async {
        SpringBoardServicesClient::connect(provider)
            .await
            .map(|_| ())
    });

    let mut client = match SpringBoardServicesClient::connect(provider).await {
        Ok(c) => c,
        Err(e) => {
            println!("  springboard: cannot connect ({e}), skipping remaining tests");
            *failure += 1;
            return;
        }
    };

    run_test!(
        "springboard: get_interface_orientation",
        success,
        failure,
        async {
            client
                .get_interface_orientation()
                .await
                .map(|o| println!("(orientation={o:?})"))
        }
    );

    run_test!(
        "springboard: get_homescreen_icon_metrics",
        success,
        failure,
        async { client.get_homescreen_icon_metrics().await.map(|_| ()) }
    );

    run_test!("springboard: get_icon_state", success, failure, async {
        client.get_icon_state(Some("2")).await.map(|_| ())
    });

    run_test!(
        "springboard: get_icon_pngdata (com.apple.Preferences)",
        success,
        failure,
        async {
            let png = client
                .get_icon_pngdata("com.apple.Preferences".to_string())
                .await?;
            if png.is_empty() {
                Err(idevice::IdeviceError::UnexpectedResponse(
                    "icon PNG was empty".into(),
                ))
            } else {
                println!("({} bytes)", png.len());
                Ok(())
            }
        }
    );
}
