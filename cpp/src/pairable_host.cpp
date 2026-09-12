// Jackson Coxson

#include <algorithm>
#include <idevice++/pairable_host.hpp>

namespace IdeviceFFI {
namespace {

Option<PeerDeviceInfo> copy_peer_device(RpPairingPeerDeviceC* peer) {
    if (peer == nullptr) {
        return Option<PeerDeviceInfo>(None);
    }

    auto str = [](const char* c) { return c != nullptr ? std::string(c) : std::string(); };
    PeerDeviceInfo info;
    info.account_id = str(peer->account_id);
    info.model      = str(peer->model);
    info.name       = str(peer->name);
    info.udid       = str(peer->udid);
    std::copy(std::begin(peer->alt_irk), std::end(peer->alt_irk), info.alt_irk.begin());
    ::rppairing_peer_device_free(peer);
    return Option<PeerDeviceInfo>(std::move(info));
}

} // namespace

Result<PairableHost, FfiError> PairableHost::prepare(const std::string& name,
                                                     const std::string& model,
                                                     bool               allows_pinless_pairing) {
    ::PairableHostHandle* handle     = nullptr;
    char*                  service_id = nullptr;
    uint8_t*               txt_data   = nullptr;
    uintptr_t              txt_len    = 0;
    std::array<uint8_t, 16> host_alt_irk{};

    FfiError e(::pairable_host_prepare(name.c_str(),
                                       model.empty() ? nullptr : model.c_str(),
                                       allows_pinless_pairing,
                                       &handle,
                                       &service_id,
                                       &txt_data,
                                       &txt_len,
                                       host_alt_irk.data()));
    if (e) {
        ::pairable_host_free(handle);
        ::idevice_string_free(service_id);
        ::idevice_data_free(txt_data, txt_len);
        return Err(e);
    }

    std::string service = service_id != nullptr ? std::string(service_id) : std::string();
    ::idevice_string_free(service_id);

    std::vector<uint8_t> txt;
    if (txt_data != nullptr && txt_len > 0) {
        txt.assign(txt_data, txt_data + txt_len);
    }
    ::idevice_data_free(txt_data, txt_len);

    return Ok(PairableHost(handle, std::move(service), std::move(txt), host_alt_irk));
}

#if defined(__unix__) || defined(__APPLE__)
Result<PairableHostResult, FfiError> PairableHost::accept_fd(int fd,
                                                              PinDisplayCallback pin_callback,
                                                              void*              pin_context) {
    RpPairingPeerDeviceC* peer = nullptr;
    RpPairingFileHandle*  out  = nullptr;
    FfiError              e(::pairable_host_accept_fd(handle_.get(),
                                                       fd,
                                                       pin_callback,
                                                       pin_context,
                                                       &peer,
                                                       &out));
    if (e) {
        ::rppairing_peer_device_free(peer);
        ::rp_pairing_file_free(out);
        return Err(e);
    }

    return Ok(PairableHostResult{RpPairingFile::adopt(out), host_alt_irk_, copy_peer_device(peer)});
}
#endif


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
                                                    const PairableHostCancelToken* cancel,
                                                    bool                           allows_pinless_pairing) {
    RpPairingFileHandle*    out  = nullptr;
    RpPairingPeerDeviceC*   peer = nullptr;
    std::array<uint8_t, 16> host_alt_irk{};
    FfiError                e(::pairable_host_accept_with_options(
        name.c_str(),
        model.empty() ? nullptr : model.c_str(),
        port,
        allows_pinless_pairing,
        pin_callback,
        pin_context,
        cancel != nullptr ? cancel->raw() : nullptr,
        host_alt_irk.data(),
        &peer,
        &out));
    if (e) {
        return Err(e);
    }

    return Ok(PairableHostResult{RpPairingFile::adopt(out), host_alt_irk, copy_peer_device(peer)});
}

} // namespace IdeviceFFI
