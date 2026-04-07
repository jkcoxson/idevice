// Jackson Coxson

use crate::run_test;
use idevice::{
    IdeviceService, provider::IdeviceProvider,
    services::notification_proxy::NotificationProxyClient,
};

pub async fn run_tests(provider: &dyn IdeviceProvider, success: &mut u32, failure: &mut u32) {
    run_test!("notification_proxy: connect", success, failure, async {
        NotificationProxyClient::connect(provider).await.map(|_| ())
    });

    let mut client = match NotificationProxyClient::connect(provider).await {
        Ok(c) => c,
        Err(e) => {
            println!("  notification_proxy: cannot connect ({e}), skipping remaining tests");
            *failure += 1;
            return;
        }
    };

    // post_notification sends a trigger TO the device — the device doesn't
    // echo it back to the same observer.  Instead, observe a system notification
    // and post an unrelated one just to verify the send path.
    run_test!(
        "notification_proxy: post_notification (send path)",
        success,
        failure,
        async {
            // post_notification is fire-and-forget; success means no error
            client.post_notification("com.apple.language.changed").await
        }
    );

    run_test!(
        "notification_proxy: observe system notifications",
        success,
        failure,
        async {
            // Register for several known system notifications
            client
                .observe_notifications(&[
                    "com.apple.springboard.lockstate",
                    "com.apple.springboard.hasBlankedScreen",
                    "com.apple.springboard.volChanged",
                ])
                .await
        }
    );
}
