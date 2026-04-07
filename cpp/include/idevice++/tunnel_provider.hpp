// Jackson Coxson

#pragma once
#include <idevice++/bindings.hpp>
#include <idevice++/core_device_proxy.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/provider.hpp>
#include <idevice++/rp_pairing_file.hpp>
#include <idevice++/rsd.hpp>
#include <string>
#include <utility>

namespace IdeviceFFI {

struct UsbTunnelResult {
    Adapter      adapter;
    RsdHandshake handshake;
};

/// PIN callback type for Apple TV / Vision Pro pairing.
/// For iOS, pass nullptr (defaults to "000000").
/// The returned string must be null-terminated and valid until the next call.
using PinCallback = const char* (*)(void* context);

/// Creates an RSD tunnel over USB via CoreDeviceProxy.
/// No need to stop remoted.
Result<UsbTunnelResult, FfiError> create_usb_tunnel(Provider& provider);

/// Pairs with a device via USB CoreDeviceProxy tunnel and returns an RPPairing file.
/// The user will need to tap "Trust" on the device.
/// For iOS, pass nullptr for pin_callback. For Apple TV / Vision Pro, provide a callback.
Result<RpPairingFile, FfiError> pair_usb(Provider&          provider,
                                         const std::string& hostname,
                                         PinCallback        pin_callback = nullptr,
                                         void*              pin_context  = nullptr);

/// Creates a tunnel over the network via RemoteXPC.
/// Use for devices discovered via _remoted._tcp (NCM / USB Ethernet).
Result<UsbTunnelResult, FfiError> create_remotexpc_tunnel(const idevice_sockaddr* addr,
                                                          idevice_socklen_t       addr_len,
                                                          const std::string&      hostname,
                                                          RpPairingFile&          pairing_file,
                                                          PinCallback             pin_callback = nullptr,
                                                          void*                   pin_context  = nullptr);

/// Creates a tunnel over the network via raw RPPairing protocol.
/// Use for devices discovered via _remotepairing._tcp (Wi-Fi / LAN).
Result<UsbTunnelResult, FfiError> create_rppairing_tunnel(const idevice_sockaddr* addr,
                                                          idevice_socklen_t       addr_len,
                                                          const std::string&      hostname,
                                                          RpPairingFile&          pairing_file,
                                                          PinCallback             pin_callback = nullptr,
                                                          void*                   pin_context  = nullptr);

} // namespace IdeviceFFI
