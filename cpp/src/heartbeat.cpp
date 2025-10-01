// Jackson Coxson

#include <idevice++/bindings.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/heartbeat.hpp>
#include <idevice++/provider.hpp>
#include <sys/_types/_u_int64_t.h>

namespace IdeviceFFI {

Result<Heartbeat, FfiError> Heartbeat::connect(Provider& provider) {
    HeartbeatClientHandle* out = nullptr;
    FfiError               e(::heartbeat_connect(provider.raw(), &out));
    if (e) {
        provider.release();
        return Err(e);
    }
    return Ok(Heartbeat::adopt(out));
}

Result<Heartbeat, FfiError> Heartbeat::from_socket(Idevice&& socket) {
    HeartbeatClientHandle* out = nullptr;
    FfiError               e(::heartbeat_new(socket.raw(), &out));
    if (e) {
        return Err(e);
    }
    socket.release();
    return Ok(Heartbeat::adopt(out));
}

Result<void, FfiError> Heartbeat::send_polo() {
    FfiError e(::heartbeat_send_polo(handle_.get()));
    if (e) {
        return Err(e);
    }
    return Ok();
}

Result<u_int64_t, FfiError> Heartbeat::get_marco(u_int64_t interval) {
    u_int64_t new_interval = 0;
    FfiError  e(::heartbeat_get_marco(handle_.get(), interval, &new_interval));
    if (e) {
        return Err(e);
    }
    return Ok(new_interval);
}

} // namespace IdeviceFFI
