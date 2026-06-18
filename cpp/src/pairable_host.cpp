// Jackson Coxson

#include <idevice++/pairable_host.hpp>

namespace IdeviceFFI {

Result<PairableHostResult, FfiError> accept_pairing(const std::string& name,
                                                    const std::string& model,
                                                    uint16_t           port,
                                                    PinDisplayCallback pin_callback,
                                                    void*              pin_context) {
    RpPairingFileHandle*    out = nullptr;
    std::array<uint8_t, 16> host_alt_irk{};
    FfiError                e(::pairable_host_accept(name.c_str(),
                                                     model.empty() ? nullptr : model.c_str(),
                                                     port,
                                                     pin_callback,
                                                     pin_context,
                                                     host_alt_irk.data(),
                                                     &out));
    if (e) {
        return Err(e);
    }
    return Ok(PairableHostResult{RpPairingFile::adopt(out), host_alt_irk});
}

} // namespace IdeviceFFI
