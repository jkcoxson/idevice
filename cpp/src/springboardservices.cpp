// Jackson Coxson

#include <cstdlib>
#include <idevice++/bindings.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/provider.hpp>
#include <idevice++/springboardservices.hpp>

namespace IdeviceFFI {

Result<SpringBoardServices, FfiError> SpringBoardServices::connect(Provider& provider) {
    SpringBoardServicesClientHandle* out = nullptr;
    FfiError                         e(::springboard_services_connect(provider.raw(), &out));
    if (e) {
        provider.release();
        return Err(e);
    }
    return Ok(SpringBoardServices::adopt(out));
}

Result<SpringBoardServices, FfiError> SpringBoardServices::connect_rsd(AdapterHandle* adapter, RsdHandshakeHandle* handshake) {
    SpringBoardServicesClientHandle* out = nullptr;
    FfiError                         e(::springboard_services_connect_rsd(adapter, handshake, &out));
    if (e) {
        return Err(e);
    }
    return Ok(SpringBoardServices::adopt(out));
}

Result<SpringBoardServices, FfiError> SpringBoardServices::from_socket(Idevice&& socket) {
    SpringBoardServicesClientHandle* out = nullptr;
    FfiError                         e(::springboard_services_new(socket.raw(), &out));
    if (e) {
        return Err(e);
    }
    socket.release();
    return Ok(SpringBoardServices::adopt(out));
}

Result<std::vector<uint8_t>, FfiError>
SpringBoardServices::get_icon(const std::string& bundle_identifier) {
    void*    data = nullptr;
    size_t   len  = 0;
    FfiError e(
        ::springboard_services_get_icon(handle_.get(), bundle_identifier.c_str(), &data, &len));
    if (e) {
        return Err(e);
    }

    std::vector<uint8_t> result;
    if (data && len > 0) {
        auto* bytes = static_cast<uint8_t*>(data);
        result.assign(bytes, bytes + len);
        ::idevice_data_free(bytes, len);
    }

    return Ok(std::move(result));
}

Result<std::vector<uint8_t>, FfiError> SpringBoardServices::get_home_screen_wallpaper_preview() {
    void*    data = nullptr;
    size_t   len  = 0;
    FfiError e(
        ::springboard_services_get_home_screen_wallpaper_preview(handle_.get(), &data, &len));
    if (e) {
        return Err(e);
    }

    std::vector<uint8_t> result;
    if (data && len > 0) {
        auto* bytes = static_cast<uint8_t*>(data);
        result.assign(bytes, bytes + len);
        ::idevice_data_free(bytes, len);
    }

    return Ok(std::move(result));
}

Result<std::vector<uint8_t>, FfiError> SpringBoardServices::get_lock_screen_wallpaper_preview() {
    void*    data = nullptr;
    size_t   len  = 0;
    FfiError e(
        ::springboard_services_get_lock_screen_wallpaper_preview(handle_.get(), &data, &len));
    if (e) {
        return Err(e);
    }

    std::vector<uint8_t> result;
    if (data && len > 0) {
        auto* bytes = static_cast<uint8_t*>(data);
        result.assign(bytes, bytes + len);
        ::idevice_data_free(bytes, len);
    }

    return Ok(std::move(result));
}

Result<uint8_t, FfiError> SpringBoardServices::get_interface_orientation() {
    uint8_t  orientation = 0;
    FfiError e(::springboard_services_get_interface_orientation(handle_.get(), &orientation));
    if (e) {
        return Err(e);
    }
    return Ok(orientation);
}

Result<plist_t, FfiError> SpringBoardServices::get_homescreen_icon_metrics() {
    plist_t  res = nullptr;
    FfiError e(::springboard_services_get_homescreen_icon_metrics(handle_.get(), &res));
    if (e) {
        return Err(e);
    }
    return Ok(res);
}

} // namespace IdeviceFFI
