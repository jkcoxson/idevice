// Jackson Coxson

#pragma once
#include <cstddef>
#include <cstdint>
#include <idevice++/adapter_stream.hpp>
#include <idevice++/core_device_proxy.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/readwrite.hpp>
#include <idevice++/rsd.hpp>
#include <memory>
#include <string>
#include <vector>

namespace IdeviceFFI {

using IconServicePtr =
    std::unique_ptr<IconServiceHandle, FnDeleter<IconServiceHandle, icon_service_free>>;

// An app icon, rendered by the device.
struct AppIcon {
    // PNG-encoded image data
    std::vector<uint8_t> png_data;
    // Dimensions in pixels, i.e. the points multiplied by the scale
    double               pixel_width{};
    double               pixel_height{};
    // Dimensions in points, as actually rendered. May be smaller than what was
    // requested.
    double               width{};
    double               height{};
    double               scale{};
    // True when the device had no real icon for the app and rendered a generic
    // placeholder instead.
    bool                 is_placeholder{};
};

class IconService {
  public:
    // Factory: connect via RSD (borrows adapter & handshake)
    static Result<IconService, FfiError> connect_rsd(Adapter& adapter, RsdHandshake& rsd);

    // Factory: from socket Box<dyn ReadWrite> (consumes it).
    static Result<IconService, FfiError> from_readwrite_ptr(ReadWriteOpaque* consumed);

    // nice ergonomic overload: consume a C++ ReadWrite by releasing it
    static Result<IconService, FfiError> from_readwrite(ReadWrite&& rw);

    // API
    Result<AppIcon, FfiError>            fetch_icon(const std::string& bundle_id,
                                                    float               width,
                                                    float               height,
                                                    float               scale,
                                                    bool                allow_placeholder);

    // Same, for an app named by its path on the device instead of its bundle ID
    Result<AppIcon, FfiError>            fetch_icon_for_path(const std::string& app_path,
                                                             float              width,
                                                             float              height,
                                                             float              scale,
                                                             bool               allow_placeholder);

    // RAII / moves
    ~IconService() noexcept                          = default;
    IconService(IconService&&) noexcept              = default;
    IconService& operator=(IconService&&) noexcept   = default;
    IconService(const IconService&)                  = delete;
    IconService&       operator=(const IconService&) = delete;

    IconServiceHandle* raw() const noexcept { return handle_.get(); }
    static IconService adopt(IconServiceHandle* h) noexcept { return IconService(h); }

  private:
    explicit IconService(IconServiceHandle* h) noexcept : handle_(h) {}
    IconServicePtr handle_{};
};

} // namespace IdeviceFFI
