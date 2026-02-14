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

Result<void, FfiError> NotificationProxy::observe_notifications(const std::vector<std::string>& names) {
    std::vector<const char*> ptrs;
    ptrs.reserve(names.size() + 1);
    for (const auto& n : names) {
        ptrs.push_back(n.c_str());
    }
    ptrs.push_back(nullptr);
    FfiError e(::notification_proxy_observe_multiple(handle_.get(), ptrs.data()));
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

Result<std::string, FfiError> NotificationProxy::receive_notification_with_timeout(u_int64_t interval) {
    char*    name_ptr = nullptr;
    FfiError e(::notification_proxy_receive_with_timeout(handle_.get(), interval, &name_ptr));
    if (e) {
        return Err(e);
    }
    std::string name(name_ptr);
    ::notification_proxy_free_string(name_ptr);
    return Ok(std::move(name));
}

} // namespace IdeviceFFI
