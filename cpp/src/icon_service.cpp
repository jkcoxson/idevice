// Jackson Coxson

#include <idevice++/icon_service.hpp>

namespace IdeviceFFI {

// ---- Factories ----
Result<IconService, FfiError> IconService::connect_rsd(Adapter& adapter, RsdHandshake& rsd) {
    IconServiceHandle* out = nullptr;
    if (IdeviceFfiError* e = ::icon_service_connect_rsd(adapter.raw(), rsd.raw(), &out)) {
        return Err(FfiError(e));
    }
    return Ok(IconService::adopt(out));
}

Result<IconService, FfiError> IconService::from_readwrite_ptr(ReadWriteOpaque* consumed) {
    IconServiceHandle* out = nullptr;
    if (IdeviceFfiError* e = ::icon_service_new(consumed, &out)) {
        return Err(FfiError(e));
    }
    return Ok(IconService::adopt(out));
}

Result<IconService, FfiError> IconService::from_readwrite(ReadWrite&& rw) {
    // Rust consumes the stream regardless of result → release BEFORE call
    return from_readwrite_ptr(rw.release());
}

// ---- Helpers ----
static AppIcon copy_and_free_icon(AppIconC* c) {
    AppIcon out;
    if (c->png_data && c->png_data_len) {
        out.png_data.assign(c->png_data, c->png_data + c->png_data_len);
    }
    out.pixel_width    = c->pixel_width;
    out.pixel_height   = c->pixel_height;
    out.width          = c->width;
    out.height         = c->height;
    out.scale          = c->scale;
    out.is_placeholder = c->is_placeholder != 0;
    ::icon_service_free_icon(c);
    return out;
}

// ---- API impls ----
Result<AppIcon, FfiError> IconService::fetch_icon(const std::string& bundle_id,
                                                  float              width,
                                                  float              height,
                                                  float              scale,
                                                  bool               allow_placeholder) {
    AppIconC* c = nullptr;
    if (IdeviceFfiError* e = ::icon_service_fetch_icon(handle_.get(),
                                                       bundle_id.c_str(),
                                                       nullptr,
                                                       width,
                                                       height,
                                                       scale,
                                                       allow_placeholder ? 1 : 0,
                                                       &c)) {
        return Err(FfiError(e));
    }
    return Ok(copy_and_free_icon(c));
}

Result<AppIcon, FfiError> IconService::fetch_icon_for_path(const std::string& app_path,
                                                           float              width,
                                                           float              height,
                                                           float              scale,
                                                           bool               allow_placeholder) {
    AppIconC* c = nullptr;
    if (IdeviceFfiError* e = ::icon_service_fetch_icon(handle_.get(),
                                                       nullptr,
                                                       app_path.c_str(),
                                                       width,
                                                       height,
                                                       scale,
                                                       allow_placeholder ? 1 : 0,
                                                       &c)) {
        return Err(FfiError(e));
    }
    return Ok(copy_and_free_icon(c));
}

} // namespace IdeviceFFI
