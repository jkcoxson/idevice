// Jackson Coxson

#include <algorithm>
#include <idevice++/pairable_host.hpp>

namespace IdeviceFFI {

Option<PairableHostCancelToken> PairableHostCancelToken::create() noexcept {
    ::PairableHostCancel* h = ::pairable_host_cancel_new();
    if (h == nullptr) {
        return Option<PairableHostCancelToken>(None);
    }
    return Option<PairableHostCancelToken>(PairableHostCancelToken(h));
}

void PairableHostCancelToken::signal() const noexcept {
    ::pairable_host_cancel_signal(handle_.get());
}

Result<PairableHostResult, FfiError> accept_pairing(const std::string&             name,
                                                    const std::string&             model,
                                                    uint16_t                       port,
                                                    PinDisplayCallback             pin_callback,
                                                    void*                          pin_context,
                                                    const PairableHostCancelToken* cancel) {
    RpPairingFileHandle*    out  = nullptr;
    RpPairingPeerDeviceC*   peer = nullptr;
    std::array<uint8_t, 16> host_alt_irk{};
    FfiError                e(::pairable_host_accept(name.c_str(),
                                                     model.empty() ? nullptr : model.c_str(),
                                                     port,
                                                     pin_callback,
                                                     pin_context,
                                                     cancel != nullptr ? cancel->raw() : nullptr,
                                                     host_alt_irk.data(),
                                                     &peer,
                                                     &out));
    if (e) {
        return Err(e);
    }

    Option<PeerDeviceInfo> peer_info(None);
    if (peer != nullptr) {
        auto str = [](const char* c) { return c != nullptr ? std::string(c) : std::string(); };
        PeerDeviceInfo info;
        info.account_id = str(peer->account_id);
        info.model      = str(peer->model);
        info.name       = str(peer->name);
        info.udid       = str(peer->udid);
        std::copy(std::begin(peer->alt_irk), std::end(peer->alt_irk), info.alt_irk.begin());
        ::rppairing_peer_device_free(peer);
        peer_info = Option<PeerDeviceInfo>(std::move(info));
    }

    return Ok(PairableHostResult{RpPairingFile::adopt(out), host_alt_irk, std::move(peer_info)});
}

} // namespace IdeviceFFI
