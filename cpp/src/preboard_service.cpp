// Jackson Coxson

#include <idevice++/bindings.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/preboard_service.hpp>
#include <idevice++/provider.hpp>

namespace IdeviceFFI {

Result<PreboardService, FfiError> PreboardService::connect(Provider& provider) {
    PreboardServiceClientHandle* out = nullptr;
    FfiError                     e(::preboard_service_connect(provider.raw(), &out));
    if (e) {
        provider.release();
        return Err(e);
    }
    return Ok(PreboardService::adopt(out));
}

Result<PreboardService, FfiError> PreboardService::connect_rsd(AdapterHandle* adapter, RsdHandshakeHandle* handshake) {
    PreboardServiceClientHandle* out = nullptr;
    FfiError                     e(::preboard_service_connect_rsd(adapter, handshake, &out));
    if (e) {
        return Err(e);
    }
    return Ok(PreboardService::adopt(out));
}

Result<PreboardService, FfiError> PreboardService::from_socket(Idevice&& socket) {
    PreboardServiceClientHandle* out = nullptr;
    FfiError                     e(::preboard_service_new(socket.raw(), &out));
    if (e) {
        return Err(e);
    }
    socket.release();
    return Ok(PreboardService::adopt(out));
}

Result<void, FfiError> PreboardService::create_stashbag(const std::vector<uint8_t>& manifest) {
    FfiError e(::preboard_service_create_stashbag(handle_.get(), manifest.data(), manifest.size()));
    if (e) {
        return Err(e);
    }
    return Ok();
}

Result<void, FfiError> PreboardService::commit_stashbag(const std::vector<uint8_t>& manifest) {
    FfiError e(::preboard_service_commit_stashbag(handle_.get(), manifest.data(), manifest.size()));
    if (e) {
        return Err(e);
    }
    return Ok();
}

} // namespace IdeviceFFI
