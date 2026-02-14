// Jackson Coxson

#include <idevice++/bindings.hpp>
#include <idevice++/diagnostics_relay.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/provider.hpp>

namespace IdeviceFFI {

// -------- Factory Methods --------

Result<DiagnosticsRelay, FfiError> DiagnosticsRelay::connect(Provider& provider) {
    DiagnosticsRelayClientHandle* out = nullptr;
    FfiError                      e(::diagnostics_relay_client_connect(provider.raw(), &out));
    if (e) {
        return Err(e);
    }
    return Ok(DiagnosticsRelay::adopt(out));
}

Result<DiagnosticsRelay, FfiError> DiagnosticsRelay::from_socket(Idevice&& socket) {
    DiagnosticsRelayClientHandle* out = nullptr;
    FfiError                      e(::diagnostics_relay_client_new(socket.raw(), &out));
    if (e) {
        return Err(e);
    }
    socket.release();
    return Ok(DiagnosticsRelay::adopt(out));
}

// -------- API Methods --------

Result<Option<plist_t>, FfiError>
DiagnosticsRelay::ioregistry(Option<std::string> current_plane,
                             Option<std::string> entry_name,
                             Option<std::string> entry_class) const {
    plist_t     res       = nullptr;

    const char* plane_ptr = current_plane.is_some() ? current_plane.unwrap().c_str() : nullptr;
    const char* name_ptr  = entry_name.is_some() ? entry_name.unwrap().c_str() : nullptr;
    const char* class_ptr = entry_class.is_some() ? entry_class.unwrap().c_str() : nullptr;

    FfiError    e(
        ::diagnostics_relay_client_ioregistry(handle_.get(), plane_ptr, name_ptr, class_ptr, &res));
    if (e) {
        return Err(e);
    }

    if (res == nullptr) {
        return Ok(Option<plist_t>(None));
    }
    return Ok(Some(res));
}

Result<Option<plist_t>, FfiError>
DiagnosticsRelay::mobilegestalt(Option<std::vector<char*>> keys) const {
    plist_t res = nullptr;

    if (!keys.is_some() || keys.unwrap().empty()) {
        return Err(FfiError::InvalidArgument());
    }

    FfiError e(::diagnostics_relay_client_mobilegestalt(
        handle_.get(), keys.unwrap().data(), keys.unwrap().size(), &res));
    if (e) {
        return Err(e);
    }

    if (res == nullptr) {
        return Ok(Option<plist_t>(None));
    }
    return Ok(Some(res));
}

Result<Option<plist_t>, FfiError> DiagnosticsRelay::gasguage() const {
    plist_t  res = nullptr;
    FfiError e(::diagnostics_relay_client_gasguage(handle_.get(), &res));
    if (e) {
        return Err(e);
    }

    if (res == nullptr) {
        return Ok(Option<plist_t>(None));
    }
    return Ok(Some(res));
}

Result<Option<plist_t>, FfiError> DiagnosticsRelay::nand() const {
    plist_t  res = nullptr;
    FfiError e(::diagnostics_relay_client_nand(handle_.get(), &res));
    if (e) {
        return Err(e);
    }

    if (res == nullptr) {
        return Ok(Option<plist_t>(None));
    }
    return Ok(Some(res));
}

Result<Option<plist_t>, FfiError> DiagnosticsRelay::all() const {
    plist_t  res = nullptr;
    FfiError e(::diagnostics_relay_client_all(handle_.get(), &res));
    if (e) {
        return Err(e);
    }

    if (res == nullptr) {
        return Ok(Option<plist_t>(None));
    }
    return Ok(Some(res));
}

Result<Option<plist_t>, FfiError> DiagnosticsRelay::wifi() const {
    plist_t  res = nullptr;
    FfiError e(::diagnostics_relay_client_wifi(handle_.get(), &res));
    if (e) {
        return Err(e);
    }

    if (res == nullptr) {
        return Ok(Option<plist_t>(None));
    }
    return Ok(Some(res));
}

Result<void, FfiError> DiagnosticsRelay::restart() {
    FfiError e(::diagnostics_relay_client_restart(handle_.get()));
    if (e) {
        return Err(e);
    }
    return Ok();
}

Result<void, FfiError> DiagnosticsRelay::shutdown() {
    FfiError e(::diagnostics_relay_client_shutdown(handle_.get()));
    if (e) {
        return Err(e);
    }
    return Ok();
}

Result<void, FfiError> DiagnosticsRelay::sleep() {
    FfiError e(::diagnostics_relay_client_sleep(handle_.get()));
    if (e) {
        return Err(e);
    }
    return Ok();
}

Result<void, FfiError> DiagnosticsRelay::goodbye() {
    FfiError e(::diagnostics_relay_client_goodbye(handle_.get()));
    if (e) {
        return Err(e);
    }
    return Ok();
}

} // namespace IdeviceFFI