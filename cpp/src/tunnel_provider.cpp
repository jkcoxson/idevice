// Jackson Coxson

#include <idevice++/tunnel_provider.hpp>

namespace IdeviceFFI {

Result<UsbTunnelResult, FfiError> create_usb_tunnel(Provider& provider) {
    AdapterHandle*      adapter   = nullptr;
    RsdHandshakeHandle* handshake = nullptr;
    FfiError            e(::rsd_tunnel_create_usb(provider.raw(), &adapter, &handshake));
    if (e) {
        return Err(e);
    }
    return Ok(UsbTunnelResult{Adapter::adopt(adapter), RsdHandshake::adopt(handshake)});
}

Result<RpPairingFile, FfiError> pair_usb(Provider&          provider,
                                         const std::string& hostname,
                                         PinCallback        pin_callback,
                                         void*              pin_context) {
    RpPairingFileHandle* out = nullptr;
    FfiError             e(::rsd_tunnel_pair_usb(provider.raw(), hostname.c_str(),
                                                  pin_callback, pin_context, &out));
    if (e) {
        return Err(e);
    }
    return Ok(RpPairingFile::adopt(out));
}

Result<UsbTunnelResult, FfiError> create_network_tunnel(const idevice_sockaddr* addr,
                                                        idevice_socklen_t       addr_len,
                                                        RpPairingFile&          pairing_file,
                                                        PinCallback             pin_callback,
                                                        void*                   pin_context) {
    AdapterHandle*      adapter   = nullptr;
    RsdHandshakeHandle* handshake = nullptr;
    FfiError            e(::rsd_tunnel_create_network(addr, addr_len, pairing_file.raw(),
                                                      pin_callback, pin_context,
                                                      &adapter, &handshake));
    if (e) {
        return Err(e);
    }
    return Ok(UsbTunnelResult{Adapter::adopt(adapter), RsdHandshake::adopt(handshake)});
}

} // namespace IdeviceFFI
