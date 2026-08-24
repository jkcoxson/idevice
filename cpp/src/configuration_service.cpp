// Jackson Coxson

#include <idevice++/configuration_service.hpp>

namespace IdeviceFFI {

// ---- Factories ----
Result<ConfigurationService, FfiError> ConfigurationService::connect_rsd(Adapter&      adapter,
                                                                         RsdHandshake& rsd) {
    ConfigurationServiceHandle* out = nullptr;
    if (IdeviceFfiError* e = ::configuration_service_connect_rsd(adapter.raw(), rsd.raw(), &out)) {
        return Err(FfiError(e));
    }
    return Ok(ConfigurationService::adopt(out));
}

Result<ConfigurationService, FfiError>
ConfigurationService::from_readwrite_ptr(ReadWriteOpaque* consumed) {
    ConfigurationServiceHandle* out = nullptr;
    if (IdeviceFfiError* e = ::configuration_service_new(consumed, &out)) {
        return Err(FfiError(e));
    }
    return Ok(ConfigurationService::adopt(out));
}

Result<ConfigurationService, FfiError> ConfigurationService::from_readwrite(ReadWrite&& rw) {
    // Rust consumes the stream regardless of result → release BEFORE call
    return from_readwrite_ptr(rw.release());
}

// ---- Appearance ----
Result<UserInterfaceStyle, FfiError> ConfigurationService::get_user_interface_style() {
    ::IdeviceUserInterfaceStyle style = IdeviceUserInterfaceStyleLight;
    if (IdeviceFfiError* e = ::configuration_service_get_user_interface_style(handle_.get(),
                                                                              &style)) {
        return Err(FfiError(e));
    }
    return Ok(static_cast<UserInterfaceStyle>(style));
}

Result<void, FfiError> ConfigurationService::set_user_interface_style(UserInterfaceStyle style) {
    if (IdeviceFfiError* e = ::configuration_service_set_user_interface_style(
            handle_.get(),
            static_cast<::IdeviceUserInterfaceStyle>(style))) {
        return Err(FfiError(e));
    }
    return Ok();
}

Result<void, FfiError> ConfigurationService::set_liquid_glass_opacity(float opacity) {
    if (IdeviceFfiError* e = ::configuration_service_set_liquid_glass_opacity(handle_.get(),
                                                                              opacity)) {
        return Err(FfiError(e));
    }
    return Ok();
}

// ---- Color filter ----
Result<ColorFilter, FfiError> ConfigurationService::get_color_filter() {
    ColorFilterC c{};
    if (IdeviceFfiError* e = ::configuration_service_get_color_filter(handle_.get(), &c)) {
        return Err(FfiError(e));
    }

    ColorFilter out;
    out.enabled = c.enabled != 0;
    if (c.filter_type) {
        out.filter_type = std::string(c.filter_type);
        ::idevice_string_free(c.filter_type);
    }
    if (c.has_intensity) {
        out.intensity = c.intensity;
    }
    return Ok(std::move(out));
}

Result<void, FfiError> ConfigurationService::set_color_filter(bool                       enabled,
                                                              const Option<std::string>& filter_type,
                                                              const Option<float>& intensity) {
    if (IdeviceFfiError* e = ::configuration_service_set_color_filter(
            handle_.get(),
            enabled ? 1 : 0,
            filter_type.is_some() ? filter_type.unwrap().c_str() : nullptr,
            intensity.is_some() ? intensity.unwrap() : 0.0f,
            intensity.is_some() ? 1 : 0)) {
        return Err(FfiError(e));
    }
    return Ok();
}

// ---- Text size ----
Result<std::string, FfiError> ConfigurationService::get_device_text_size() {
    char* size = nullptr;
    if (IdeviceFfiError* e = ::configuration_service_get_device_text_size(handle_.get(), &size)) {
        return Err(FfiError(e));
    }
    std::string out;
    if (size) {
        out = size;
        ::idevice_string_free(size);
    }
    return Ok(std::move(out));
}

Result<void, FfiError> ConfigurationService::set_device_text_size(const std::string& size) {
    if (IdeviceFfiError* e = ::configuration_service_set_device_text_size(handle_.get(),
                                                                          size.c_str())) {
        return Err(FfiError(e));
    }
    return Ok();
}

// ---- Boolean knobs ----
#define IDEVICE_CONFIGURATION_BOOL_KNOB(cpp_get, cpp_set, c_get, c_set)                            \
    Result<bool, FfiError> ConfigurationService::cpp_get() {                                       \
        int enabled = 0;                                                                           \
        if (IdeviceFfiError* e = ::c_get(handle_.get(), &enabled)) {                               \
            return Err(FfiError(e));                                                               \
        }                                                                                          \
        return Ok(enabled != 0);                                                                   \
    }                                                                                              \
                                                                                                   \
    Result<void, FfiError> ConfigurationService::cpp_set(bool enabled) {                           \
        if (IdeviceFfiError* e = ::c_set(handle_.get(), enabled ? 1 : 0)) {                        \
            return Err(FfiError(e));                                                               \
        }                                                                                          \
        return Ok();                                                                               \
    }

IDEVICE_CONFIGURATION_BOOL_KNOB(get_reduce_motion,
                                set_reduce_motion,
                                configuration_service_get_reduce_motion,
                                configuration_service_set_reduce_motion)

IDEVICE_CONFIGURATION_BOOL_KNOB(get_reduce_transparency,
                                set_reduce_transparency,
                                configuration_service_get_reduce_transparency,
                                configuration_service_set_reduce_transparency)

IDEVICE_CONFIGURATION_BOOL_KNOB(get_show_borders,
                                set_show_borders,
                                configuration_service_get_show_borders,
                                configuration_service_set_show_borders)

#undef IDEVICE_CONFIGURATION_BOOL_KNOB

Result<void, FfiError> ConfigurationService::set_increase_contrast(bool enabled) {
    if (IdeviceFfiError* e = ::configuration_service_set_increase_contrast(handle_.get(),
                                                                           enabled ? 1 : 0)) {
        return Err(FfiError(e));
    }
    return Ok();
}

} // namespace IdeviceFFI
