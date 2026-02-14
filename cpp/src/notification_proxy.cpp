// Jackson Coxson

#include <idevice++/bindings.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/notification_proxy.hpp>
#include <idevice++/provider.hpp>

namespace IdeviceFFI {

Result<NotificationProxy, FfiError> NotificationProxy::connect(Provider& provider) {
    NotificationProxyClientHandle* out = nullptr;
    FfiError               e(::notification_proxy_connect(provider.raw(), &out));
    if (e) {
        provider.release();
        return Err(e);
    }
    return Ok(NotificationProxy::adopt(out));
}

Result<NotificationProxy, FfiError> NotificationProxy::from_socket(Idevice&& socket) {
    NotificationProxyClientHandle* out = nullptr;
    FfiError               e(::notification_proxy_new(socket.raw(), &out));
    if (e) {
        return Err(e);
    }
    socket.release();
    return Ok(NotificationProxy::adopt(out));
}

Result<void, FfiError> NotificationProxy::post_notification(const std::string& name) {
    FfiError e(::notification_proxy_post(handle_.get(), name.c_str()));
    if (e) {
        return Err(e);
    }
    return Ok();
}

Result<void, FfiError> NotificationProxy::observe_notification(const std::string& name) {
    FfiError e(::notification_proxy_observe(handle_.get(), name.c_str()));
    if (e) {
        return Err(e);
    }
    return Ok();
}

Result<std::string, FfiError> NotificationProxy::receive_notification() {
    char* name_ptr = nullptr;
    FfiError e(::notification_proxy_receive(handle_.get(), &name_ptr));
    if (e) {
        return Err(e);
    }
    std::string name(name_ptr);
    ::notification_proxy_free_string(name_ptr);
    return Ok(std::move(name));
}

} // namespace IdeviceFFI
