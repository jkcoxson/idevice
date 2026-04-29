// Jackson Coxson

#include <idevice++/dvt/graphics.hpp>

namespace IdeviceFFI {

Result<Graphics, FfiError> Graphics::create(RemoteServer& server) {
    GraphicsHandle* out = nullptr;
    FfiError        e(::graphics_new(server.raw(), &out));
    if (e) return Err(e);
    return Ok(Graphics::adopt(out));
}

Result<void, FfiError> Graphics::start(double interval) {
    FfiError e(::graphics_start_sampling(handle_.get(), interval));
    if (e) return Err(e);
    return Ok();
}

Result<void, FfiError> Graphics::stop() {
    FfiError e(::graphics_stop_sampling(handle_.get()));
    if (e) return Err(e);
    return Ok();
}

Result<GraphicsSample, FfiError> Graphics::next_sample() {
    IdeviceGraphicsSample* raw = nullptr;
    FfiError               e(::graphics_next_sample(handle_.get(), &raw));
    if (e) return Err(e);

    GraphicsSample sample{};
    if (raw) {
        sample.timestamp                   = raw->timestamp;
        sample.fps                         = raw->fps;
        sample.alloc_system_memory         = raw->alloc_system_memory;
        sample.in_use_system_memory        = raw->in_use_system_memory;
        sample.in_use_system_memory_driver = raw->in_use_system_memory_driver;
        sample.gpu_bundle_name =
            raw->gpu_bundle_name ? std::string(raw->gpu_bundle_name) : std::string();
        sample.recovery_count = raw->recovery_count;
    }

    ::graphics_sample_free(raw);
    return Ok(std::move(sample));
}

} // namespace IdeviceFFI
