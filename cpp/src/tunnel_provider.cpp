// Jackson Coxson

#include <idevice++/tunnel_provider.hpp>

#include <algorithm>
#include <iterator>

namespace IdeviceFFI {

Result<UsbTunnelResult, FfiError> create_usb_tunnel(Provider& provider) {
    AdapterHandle*      adapter   = nullptr;
    RsdHandshakeHandle* handshake = nullptr;
    FfiError            e(::tunnel_create_usb(provider.raw(), &adapter, &handshake));
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
    FfiError             e(::tunnel_pair_usb(provider.raw(), hostname.c_str(),
                                              pin_callback, pin_context, &out));
    if (e) {
        return Err(e);
    }
    return Ok(RpPairingFile::adopt(out));
}

Result<UsbTunnelResult, FfiError> create_remotexpc_tunnel(const idevice_sockaddr* addr,
                                                          idevice_socklen_t       addr_len,
                                                          const std::string&      hostname,
                                                          RpPairingFile&          pairing_file,
                                                          PinCallback             pin_callback,
                                                          void*                   pin_context) {
    AdapterHandle*      adapter   = nullptr;
    RsdHandshakeHandle* handshake = nullptr;
    FfiError            e(::tunnel_create_remotexpc(addr, addr_len, hostname.c_str(),
                                                    pairing_file.raw(),
                                                    pin_callback, pin_context,
                                                    &adapter, &handshake));
    if (e) {
        return Err(e);
    }
    return Ok(UsbTunnelResult{Adapter::adopt(adapter), RsdHandshake::adopt(handshake)});
}

Result<UsbTunnelResult, FfiError> create_rppairing_tunnel(const idevice_sockaddr* addr,
                                                          idevice_socklen_t       addr_len,
                                                          const std::string&      hostname,
                                                          RpPairingFile&          pairing_file,
                                                          PinCallback             pin_callback,
                                                          void*                   pin_context) {
    AdapterHandle*      adapter   = nullptr;
    RsdHandshakeHandle* handshake = nullptr;
    FfiError            e(::tunnel_create_rppairing(addr, addr_len, hostname.c_str(),
                                                    pairing_file.raw(),
                                                    pin_callback, pin_context,
                                                    &adapter, &handshake));
    if (e) {
        return Err(e);
    }
    return Ok(UsbTunnelResult{Adapter::adopt(adapter), RsdHandshake::adopt(handshake)});
}

Result<Option<PeerDeviceInfo>, FfiError> pair_rppairing_network(const idevice_sockaddr* addr,
                                                                idevice_socklen_t       addr_len,
                                                                const std::string&      hostname,
                                                                RpPairingFile&          pairing_file,
                                                                PinCallback             pin_callback,
                                                                void*                   pin_context) {
    RpPairingPeerDeviceC* peer = nullptr;
    FfiError              e(::rppairing_pair_network(addr, addr_len, hostname.c_str(),
                                                     pairing_file.raw(), pin_callback,
                                                     pin_context, &peer));
    if (e) {
        return Err(e);
    }
    if (peer == nullptr) {
        // A pair-verify succeeded, so no peer identity was exchanged.
        return Ok(Option<PeerDeviceInfo>(None));
    }

    auto           str = [](const char* c) { return c != nullptr ? std::string(c) : std::string(); };
    PeerDeviceInfo info;
    info.account_id = str(peer->account_id);
    info.model      = str(peer->model);
    info.name       = str(peer->name);
    info.udid       = str(peer->udid);
    std::copy(std::begin(peer->alt_irk), std::end(peer->alt_irk), info.alt_irk.begin());
    ::rppairing_peer_device_free(peer);

    return Ok(Option<PeerDeviceInfo>(std::move(info)));
}

} // namespace IdeviceFFI
