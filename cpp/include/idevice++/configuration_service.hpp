// Jackson Coxson

#pragma once
#include <cstddef>
#include <cstdint>
#include <idevice++/adapter_stream.hpp>
#include <idevice++/core_device_proxy.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/option.hpp>
#include <idevice++/readwrite.hpp>
#include <idevice++/rsd.hpp>
#include <memory>
#include <string>

namespace IdeviceFFI {

using ConfigurationServicePtr = std::unique_ptr<
    ConfigurationServiceHandle,
    FnDeleter<ConfigurationServiceHandle, configuration_service_free>>;

// The system's light/dark appearance.
enum class UserInterfaceStyle {
    Light = IdeviceUserInterfaceStyleLight,
    Dark  = IdeviceUserInterfaceStyleDark,
};

// The accessibility color filter's state.
struct ColorFilter {
    bool                enabled{};
    // The filter preset's name, e.g. "Protanopia"
    Option<std::string> filter_type;
    // Filter strength, 0.0 to 1.0. Optional even when the filter is enabled.
    Option<double>      intensity;
};

// Device configuration: the service behind Xcode's appearance and accessibility
// toggles.
class ConfigurationService {
  public:
    // Factory: connect via RSD (borrows adapter & handshake)
    static Result<ConfigurationService, FfiError> connect_rsd(Adapter& adapter, RsdHandshake& rsd);

    // Factory: from socket Box<dyn ReadWrite> (consumes it).
    static Result<ConfigurationService, FfiError> from_readwrite_ptr(ReadWriteOpaque* consumed);

    // nice ergonomic overload: consume a C++ ReadWrite by releasing it
    static Result<ConfigurationService, FfiError> from_readwrite(ReadWrite&& rw);

    // Appearance
    Result<UserInterfaceStyle, FfiError>          get_user_interface_style();
    Result<void, FfiError>                        set_user_interface_style(UserInterfaceStyle style);

    // System liquid-glass opacity, 0.0 to 1.0
    Result<void, FfiError>                        set_liquid_glass_opacity(float opacity);

    // Accessibility color filter. `filter_type` is required when enabling and
    // names a preset, e.g. "Protanopia".
    Result<ColorFilter, FfiError>                 get_color_filter();
    Result<void, FfiError>                        set_color_filter(bool                       enabled,
                                                                   const Option<std::string>& filter_type,
                                                                   const Option<float>&       intensity);

    // Dynamic-type size by name, e.g. "medium" or "large"
    Result<std::string, FfiError>                 get_device_text_size();
    Result<void, FfiError>                        set_device_text_size(const std::string& size);

    // Accessibility knobs
    Result<bool, FfiError>                        get_reduce_motion();
    Result<void, FfiError>                        set_reduce_motion(bool enabled);
    Result<bool, FfiError>                        get_reduce_transparency();
    Result<void, FfiError>                        set_reduce_transparency(bool enabled);
    // The device offers no getter for Increase Contrast.
    Result<void, FfiError>                        set_increase_contrast(bool enabled);

    // Layout-debug borders overlay
    Result<bool, FfiError>                        get_show_borders();
    Result<void, FfiError>                        set_show_borders(bool enabled);

    // RAII / moves
    ~ConfigurationService() noexcept                                   = default;
    ConfigurationService(ConfigurationService&&) noexcept              = default;
    ConfigurationService& operator=(ConfigurationService&&) noexcept   = default;
    ConfigurationService(const ConfigurationService&)                  = delete;
    ConfigurationService&       operator=(const ConfigurationService&) = delete;

    ConfigurationServiceHandle* raw() const noexcept { return handle_.get(); }
    static ConfigurationService adopt(ConfigurationServiceHandle* h) noexcept {
        return ConfigurationService(h);
    }

  private:
    explicit ConfigurationService(ConfigurationServiceHandle* h) noexcept : handle_(h) {}
    ConfigurationServicePtr handle_{};
};

} // namespace IdeviceFFI
