// Jackson Coxson

#include <cstring>
#include <idevice++/debug_proxy.hpp>

namespace IdeviceFFI {

// ---- helpers ----
static std::optional<std::string> take_cstring(char* p) {
    if (!p)
        return std::nullopt;
    std::string s(p);
    ::idevice_string_free(p);
    return s;
}

// ---- DebugCommand ----
std::optional<DebugCommand> DebugCommand::make(const std::string&              name,
                                               const std::vector<std::string>& argv) {
    std::vector<const char*> c_argv;
    c_argv.reserve(argv.size());
    for (auto& a : argv)
        c_argv.push_back(a.c_str());

    auto* h = ::debugserver_command_new(
        name.c_str(),
        c_argv.empty() ? nullptr : const_cast<const char* const*>(c_argv.data()),
        c_argv.size());
    if (!h)
        return std::nullopt;
    return DebugCommand(h);
}

// ---- DebugProxy factories ----
std::optional<DebugProxy>
DebugProxy::connect_rsd(Adapter& adapter, RsdHandshake& rsd, FfiError& err) {
    ::DebugProxyHandle* out = nullptr;
    if (IdeviceFfiError* e = ::debug_proxy_connect_rsd(adapter.raw(), rsd.raw(), &out)) {
        err = FfiError(e);
        return std::nullopt;
    }
    return DebugProxy(out);
}

std::optional<DebugProxy> DebugProxy::from_readwrite_ptr(::ReadWriteOpaque* consumed,
                                                         FfiError&          err) {
    ::DebugProxyHandle* out = nullptr;
    if (IdeviceFfiError* e = ::debug_proxy_new(consumed, &out)) {
        err = FfiError(e);
        return std::nullopt;
    }
    return DebugProxy(out);
}

std::optional<DebugProxy> DebugProxy::from_readwrite(ReadWrite&& rw, FfiError& err) {
    // Rust consumes the pointer regardless of outcome; release before calling
    return from_readwrite_ptr(rw.release(), err);
}

// ---- DebugProxy API ----
std::optional<std::string> DebugProxy::send_command(const std::string&              name,
                                                    const std::vector<std::string>& argv,
                                                    FfiError&                       err) {
    auto cmd = DebugCommand::make(name, argv);
    if (!cmd) {
        // treat as invalid arg
        err.code    = -1;
        err.message = "debugserver_command_new failed";
        return std::nullopt;
    }

    char* resp_c = nullptr;
    if (IdeviceFfiError* e = ::debug_proxy_send_command(handle_, cmd->raw(), &resp_c)) {
        err = FfiError(e);
        return std::nullopt;
    }
    return take_cstring(resp_c); // may be null → std::nullopt
}

std::optional<std::string> DebugProxy::read_response(FfiError& err) {
    char* resp_c = nullptr;
    if (IdeviceFfiError* e = ::debug_proxy_read_response(handle_, &resp_c)) {
        err = FfiError(e);
        return std::nullopt;
    }
    return take_cstring(resp_c);
}

bool DebugProxy::send_raw(const std::vector<uint8_t>& data, FfiError& err) {
    if (IdeviceFfiError* e = ::debug_proxy_send_raw(handle_, data.data(), data.size())) {
        err = FfiError(e);
        return false;
    }
    return true;
}

std::optional<std::string> DebugProxy::read(std::size_t len, FfiError& err) {
    char* resp_c = nullptr;
    if (IdeviceFfiError* e = ::debug_proxy_read(handle_, len, &resp_c)) {
        err = FfiError(e);
        return std::nullopt;
    }
    return take_cstring(resp_c);
}

std::optional<std::string> DebugProxy::set_argv(const std::vector<std::string>& argv,
                                                FfiError&                       err) {
    std::vector<const char*> c_argv;
    c_argv.reserve(argv.size());
    for (auto& a : argv)
        c_argv.push_back(a.c_str());

    char* resp_c = nullptr;
    if (IdeviceFfiError* e = ::debug_proxy_set_argv(
            handle_,
            c_argv.empty() ? nullptr : const_cast<const char* const*>(c_argv.data()),
            c_argv.size(),
            &resp_c)) {
        err = FfiError(e);
        return std::nullopt;
    }
    return take_cstring(resp_c);
}

bool DebugProxy::send_ack(FfiError& err) {
    if (IdeviceFfiError* e = ::debug_proxy_send_ack(handle_)) {
        err = FfiError(e);
        return false;
    }
    return true;
}

bool DebugProxy::send_nack(FfiError& err) {
    if (IdeviceFfiError* e = ::debug_proxy_send_nack(handle_)) {
        err = FfiError(e);
        return false;
    }
    return true;
}

} // namespace IdeviceFFI
