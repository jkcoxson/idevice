// Jackson Coxson

#include <idevice++/bindings.hpp>
#include <idevice++/companion_proxy.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/provider.hpp>

namespace IdeviceFFI {

Result<CompanionProxy, FfiError> CompanionProxy::connect(Provider& provider) {
    CompanionProxyClientHandle* out = nullptr;
    FfiError                    e(::companion_proxy_connect(provider.raw(), &out));
    if (e) {
        provider.release();
        return Err(e);
    }
    return Ok(CompanionProxy::adopt(out));
}

Result<CompanionProxy, FfiError> CompanionProxy::from_socket(Idevice&& socket) {
    CompanionProxyClientHandle* out = nullptr;
    FfiError                    e(::companion_proxy_new(socket.raw(), &out));
    if (e) {
        return Err(e);
    }
    socket.release();
    return Ok(CompanionProxy::adopt(out));
}

Result<std::vector<std::string>, FfiError> CompanionProxy::get_device_registry() {
    char**    udids     = nullptr;
    uintptr_t udids_len = 0;
    FfiError  e(::companion_proxy_get_device_registry(handle_.get(), &udids, &udids_len));
    if (e) {
        return Err(e);
    }

    std::vector<std::string> result;
    if (udids && udids_len > 0) {
        result.reserve(udids_len);
        for (uintptr_t i = 0; i < udids_len; ++i) {
            if (udids[i]) {
                result.emplace_back(udids[i]);
                ::idevice_string_free(udids[i]);
            }
        }
        ::idevice_outer_slice_free(udids, 0);
    }

    return Ok(std::move(result));
}

Result<uint16_t, FfiError> CompanionProxy::start_forwarding_service_port(uint16_t port) {
    uint16_t local_port = 0;
    FfiError e(::companion_proxy_start_forwarding_service_port(handle_.get(), port, &local_port));
    if (e) {
        return Err(e);
    }
    return Ok(local_port);
}

Result<void, FfiError> CompanionProxy::stop_forwarding_service_port(uint16_t port) {
    FfiError e(::companion_proxy_stop_forwarding_service_port(handle_.get(), port));
    if (e) {
        return Err(e);
    }
    return Ok();
}

} // namespace IdeviceFFI
