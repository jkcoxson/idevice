// Jackson Coxson

#include <cstring>
#include <idevice++/bindings.hpp>
#include <idevice++/installation_proxy.hpp>
#include <vector>

namespace IdeviceFFI {

// -------- Anonymous Namespace for Helpers --------
namespace {
/**
 * @brief A C-style trampoline function to call back into a C++ std::function.
 *
 * This function is passed to the Rust FFI layer. It receives a void* context,
 * which it casts back to the original std::function object to invoke it.
 */
extern "C" void progress_trampoline(u_int64_t progress, void* context) {
    if (context) {
        auto& callback_fn = *static_cast<std::function<void(u_int64_t)>*>(context);
        callback_fn(progress);
    }
}
} // namespace

// -------- Factory Methods --------

Result<InstallationProxy, FfiError> InstallationProxy::connect(Provider& provider) {
    InstallationProxyClientHandle* handle = nullptr;
    FfiError                       e(::installation_proxy_connect(provider.raw(), &handle));
    if (e) {
        return Err(e);
    }
    return Ok(InstallationProxy::adopt(handle));
}

Result<InstallationProxy, FfiError> InstallationProxy::from_socket(Idevice&& socket) {
    InstallationProxyClientHandle* handle = nullptr;
    // The Rust FFI function consumes the socket, so we must release it from the
    // C++ RAII wrapper's control. An `Idevice::release()` method is assumed here.
    FfiError                       e(::installation_proxy_new(socket.release(), &handle));
    if (e) {
        return Err(e);
    }
    return Ok(InstallationProxy::adopt(handle));
}

// -------- Ops --------

Result<std::vector<plist_t>, FfiError>
InstallationProxy::get_apps(Option<std::string>              application_type,
                            Option<std::vector<std::string>> bundle_identifiers) {
    plist_t*    apps_raw             = nullptr;
    size_t      apps_len             = 0;

    const char* application_type_ptr = NULL;
    if (application_type.is_some()) {
        application_type_ptr = application_type.unwrap().c_str();
    }

    std::vector<const char*> c_bundle_id;
    size_t                   bundle_identifiers_len = 0;
    if (bundle_identifiers.is_some()) {
        c_bundle_id.reserve(bundle_identifiers.unwrap().size());
        for (auto& a : bundle_identifiers.unwrap()) {
            c_bundle_id.push_back(a.c_str());
        }
    }

    FfiError e(::installation_proxy_get_apps(
        this->raw(),
        application_type_ptr,
        c_bundle_id.empty() ? nullptr : const_cast<const char* const*>(c_bundle_id.data()),
        bundle_identifiers_len,
        apps_raw,
        &apps_len));
    if (e) {
        return Err(e);
    }

    std::vector<plist_t> apps;
    if (apps_raw) {
        apps.assign(apps_raw, apps_raw + apps_len);
    }

    return Ok(std::move(apps));
}

Result<void, FfiError> InstallationProxy::install(std::string     package_path,
                                                  Option<plist_t> options) {
    plist_t unwrapped_options;
    if (options.is_some()) {
        unwrapped_options = std::move(options).unwrap();
    } else {
        unwrapped_options = NULL;
    }

    FfiError e(::installation_proxy_install(this->raw(), package_path.c_str(), &unwrapped_options));
    if (e) {
        return Err(e);
    }
    return Ok();
}

Result<void, FfiError> InstallationProxy::install_with_callback(
    std::string package_path, Option<plist_t> options, std::function<void(u_int64_t)>& lambda

) {
    plist_t unwrapped_options;
    if (options.is_some()) {
        unwrapped_options = std::move(options).unwrap();
    } else {
        unwrapped_options = NULL;
    }

    FfiError e(::installation_proxy_install_with_callback(
        this->raw(), package_path.c_str(), &unwrapped_options, progress_trampoline, &lambda));
    if (e) {
        return Err(e);
    }
    return Ok();
}

Result<void, FfiError> InstallationProxy::upgrade(std::string     package_path,
                                                  Option<plist_t> options) {
    plist_t unwrapped_options;
    if (options.is_some()) {
        unwrapped_options = std::move(options).unwrap();
    } else {
        unwrapped_options = NULL;
    }

    FfiError e(::installation_proxy_upgrade(this->raw(), package_path.c_str(), &unwrapped_options));
    if (e) {
        return Err(e);
    }
    return Ok();
}

Result<void, FfiError> InstallationProxy::upgrade_with_callback(
    std::string package_path, Option<plist_t> options, std::function<void(u_int64_t)>& lambda

) {
    plist_t unwrapped_options;
    if (options.is_some()) {
        unwrapped_options = std::move(options).unwrap();
    } else {
        unwrapped_options = NULL;
    }

    FfiError e(::installation_proxy_upgrade_with_callback(
        this->raw(), package_path.c_str(), &unwrapped_options, progress_trampoline, &lambda));
    if (e) {
        return Err(e);
    }
    return Ok();
}

Result<void, FfiError> InstallationProxy::uninstall(std::string     package_path,
                                                    Option<plist_t> options) {
    plist_t unwrapped_options;
    if (options.is_some()) {
        unwrapped_options = std::move(options).unwrap();
    } else {
        unwrapped_options = NULL;
    }

    FfiError e(
        ::installation_proxy_uninstall(this->raw(), package_path.c_str(), &unwrapped_options));
    if (e) {
        return Err(e);
    }
    return Ok();
}

Result<void, FfiError> InstallationProxy::uninstall_with_callback(
    std::string package_path, Option<plist_t> options, std::function<void(u_int64_t)>& lambda

) {
    plist_t unwrapped_options;
    if (options.is_some()) {
        unwrapped_options = std::move(options).unwrap();
    } else {
        unwrapped_options = NULL;
    }

    FfiError e(::installation_proxy_uninstall_with_callback(
        this->raw(), package_path.c_str(), &unwrapped_options, progress_trampoline, &lambda));
    if (e) {
        return Err(e);
    }
    return Ok();
}

Result<bool, FfiError>
InstallationProxy::check_capabilities_match(std::vector<plist_t> capabilities,
                                            Option<plist_t>      options) {
    plist_t unwrapped_options;
    if (options.is_some()) {
        unwrapped_options = std::move(options).unwrap();
    } else {
        unwrapped_options = NULL;
    }

    bool     res = false;
    FfiError e(::installation_proxy_check_capabilities_match(
        this->raw(),
        capabilities.empty() ? nullptr : capabilities.data(),
        capabilities.size(),
        unwrapped_options,
        &res));
    return e ? Result<bool, FfiError>(Err(e)) : Result<bool, FfiError>(Ok(res));
}

Result<std::vector<plist_t>, FfiError> InstallationProxy::browse(Option<plist_t> options) {
    plist_t* apps_raw = nullptr;
    size_t   apps_len = 0;

    plist_t  unwrapped_options;
    if (options.is_some()) {
        unwrapped_options = std::move(options).unwrap();
    } else {
        unwrapped_options = NULL;
    }

    FfiError e(::installation_proxy_browse(this->raw(), unwrapped_options, &apps_raw, &apps_len));
    if (e) {
        return Err(e);
    }

    std::vector<plist_t> apps;
    if (apps_raw) {
        apps.assign(apps_raw, apps_raw + apps_len);
    }

    return Ok(std::move(apps));
}

} // namespace IdeviceFFI
