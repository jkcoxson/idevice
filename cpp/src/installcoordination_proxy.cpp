// Jackson Coxson

#include <idevice++/bindings.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/installcoordination_proxy.hpp>

namespace IdeviceFFI {

Result<InstallcoordinationProxy, FfiError>
InstallcoordinationProxy::connect_rsd(AdapterHandle* adapter, RsdHandshakeHandle* handshake) {
    InstallcoordinationProxyHandle* out = nullptr;
    FfiError                        e(::installcoordination_proxy_connect_rsd(adapter, handshake, &out));
    if (e) {
        return Err(e);
    }
    return Ok(InstallcoordinationProxy::adopt(out));
}

Result<InstallcoordinationProxy, FfiError>
InstallcoordinationProxy::from_readwrite_ptr(ReadWriteOpaque* consumed) {
    InstallcoordinationProxyHandle* out = nullptr;
    if (IdeviceFfiError* e = ::installcoordination_proxy_new(consumed, &out)) {
        return Err(FfiError(e));
    }
    return Ok(InstallcoordinationProxy::adopt(out));
}

Result<InstallcoordinationProxy, FfiError>
InstallcoordinationProxy::from_readwrite(ReadWrite&& rw) {
    return from_readwrite_ptr(rw.release());
}

Result<void, FfiError> InstallcoordinationProxy::uninstall_app(const std::string& bundle_id) {
    FfiError e(::installcoordination_proxy_uninstall_app(handle_.get(), bundle_id.c_str()));
    if (e) {
        return Err(e);
    }
    return Ok();
}

Result<std::string, FfiError>
InstallcoordinationProxy::query_app_path(const std::string& bundle_id) {
    char*    path = nullptr;
    FfiError e(::installcoordination_proxy_query_app_path(handle_.get(), bundle_id.c_str(), &path));
    if (e) {
        return Err(e);
    }

    std::string result;
    if (path) {
        result = path;
        ::idevice_string_free(path);
    }

    return Ok(std::move(result));
}

} // namespace IdeviceFFI
