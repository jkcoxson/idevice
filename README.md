# idevice

A Rust library for interacting with iOS services.
Inspired by [libimobiledevice](https://github.com/libimobiledevice/libimobiledevice)
and [pymobiledevice3](https://github.com/doronz88/pymobiledevice3),
this library interfaces with lockdownd and usbmuxd to perform actions
on an iOS device that a Mac normally would.

For help and information, join the [idevice Discord](https://discord.gg/qtgv6QtYbV)

## State

**IMPORTANT**: Breaking changes will happen at each point release until 0.2.0.
Pin your `Cargo.toml` to a specific version to avoid breakage.

This library is in development and research stage.
Releases are being published to crates.io for use in other projects,
but the API and feature-set are far from final or even planned.

## Features

To keep dependency bloat and compile time down, everything is contained in features.

- afc - Apple File Conduit, partial/in-progress support
- core_device_proxy - Start a secure tunnel to access protected services
- debug_proxy - Send GDB commands
- dvt - Developer tools/instruments
- heartbeat - Heartbeat the device
- installation_proxy - Install/manage apps, partial support
- springboardservices - Manage the sprinboard, partial support
- misagent - Manage provisioning profiles
- mobile_image_mounter - Manage the DDI mounted on the device
- location_simulation - Simulate the GPS location
- tcp - Connect to devices over TCP
- tunnel_tcp_stack - Naive software TCP stack for core_device_proxy
- tss - Requests to Apple's TSS servers, partial support
- tunneld - Interface with pymobiledevice3's tunneld
- usbmuxd - Connect to devices over usbmuxd daemon
- xpc - Get protected services over RSD via XPC
- full (all features)

### Planned/TODO

Finish the following:

- lockdown support
- afc
- installation_proxy
- sprinboard

Implement the following:

- amfi
- companion_proxy
- crash_reports
- diagnostics
- house_arrest
- mobilebackup2
- notification_proxy
- screenshot
- syslog_relay
- webinspector

As this project is done in my free time within my busy schedule, there
is no ETA for any of these. Feel free to contribute or donate!

## Usage

idevice is purposefully verbose to allow for powerful configurations.
No size fits all, but effort is made to reduce boilerplate via providers.

```rust
// enable the usbmuxd feature
use idevice::{lockdown::LockdowndClient, IdeviceService};
use idevice::usbmuxd::{UsbmuxdAddr, UsbmuxdConnection},

#[tokio::main]
async fn main() {
    // usbmuxd is Apple's daemon for connecting to devices over USB.
    // We'll ask usbmuxd for a device
    let mut usbmuxd = UsbmuxdConnection::default()
        .await
        .expect("Unable to connect to usbmxud")
    let devs = usbmuxd.get_devices().unwrap();
    if devs.is_empty() {
        eprintln!("No devices connected!");
        return;
    }

    // Create a provider to automatically create connections to the device.
    // Many services require opening multiple connections to get where you want.
    let provider = devs[0].to_provider(UsbmuxdAddr::from_env_var().unwrap(), 0, "example-program")

    // ``connect`` takes an object with the provider trait
    let mut lockdown_client = match LockdowndClient::connect(&provider).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Unable to connect to lockdown: {e:?}");
            return;
        }
    };

    println!("{:?}", lockdown_client.get_value("ProductVersion").await);
    println!(
        "{:?}",
        lockdown_client
            .start_session(
                &provider
                    .get_pairing_file()
                    .await
                    .expect("failed to get pairing file")
            )
            .await
    );
    println!("{:?}", lockdown_client.idevice.get_type().await.unwrap());
    println!("{:#?}", lockdown_client.get_all_values().await);
}
```

More examples are in the ``tools`` crate and in the crate documentation.

## FFI

For use in other languages, a small FFI crate has been created to start exposing
idevice. Example C programs can be found in this repository.

## Version Policy

As Apple prohibits downgrading to older versions, this library will
not keep compatibility for older versions than the current stable release.

## Developer Disk Images

doronz88 is kind enough to maintain a [repo](https://github.com/doronz88/DeveloperDiskImage)
for disk images and personalized images.
On MacOS, you can find them at ``~/Library/Developer/DeveloperDiskImages``.

## License

MIT
