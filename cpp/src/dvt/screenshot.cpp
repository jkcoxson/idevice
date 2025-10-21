// Jackson Coxson

#include <idevice++/dvt/screenshot.hpp>

namespace IdeviceFFI {

Result<ScreenshotClient, FfiError> ScreenshotClient::create(RemoteServer& server) {
    ScreenshotClientHandle* out = nullptr;
    FfiError                e(::screenshot_client_new(server.raw(), &out));
    if (e) {
        return Err(e);
    }
    return Ok(ScreenshotClient::adopt(out));
}

Result<std::vector<uint8_t>, FfiError> ScreenshotClient::take_screenshot() {
    uint8_t* data = nullptr;
    size_t   len  = 0;

    FfiError e(::screenshot_client_take_screenshot(handle_.get(), &data, &len));
    if (e) {
        return Err(e);
    }

    // Copy into a C++ buffer
    std::vector<uint8_t> out(len);
    if (len > 0 && data != nullptr) {
        std::memcpy(out.data(), data, len);
    }

    // Free Rust-allocated data
    ::idevice_data_free(data, len);

    return Ok(std::move(out));
}

} // namespace IdeviceFFI
