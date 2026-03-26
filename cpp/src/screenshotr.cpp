// Jackson Coxson

#include <idevice++/bindings.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/provider.hpp>
#include <idevice++/screenshotr.hpp>

namespace IdeviceFFI {

Result<Screenshotr, FfiError> Screenshotr::connect(Provider& provider) {
    ScreenshotrClientHandle* out = nullptr;
    FfiError                 e(::screenshotr_connect(provider.raw(), &out));
    if (e) {
        provider.release();
        return Err(e);
    }
    return Ok(Screenshotr::adopt(out));
}

Result<std::vector<uint8_t>, FfiError> Screenshotr::take_screenshot() {
    ScreenshotData screenshot{};
    FfiError       e(::screenshotr_take_screenshot(handle_.get(), &screenshot));
    if (e) {
        return Err(e);
    }

    std::vector<uint8_t> result;
    if (screenshot.data && screenshot.length > 0) {
        result.assign(screenshot.data, screenshot.data + screenshot.length);
    }
    ::screenshotr_screenshot_free(screenshot);

    return Ok(std::move(result));
}

} // namespace IdeviceFFI
