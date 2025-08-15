// Jackson Coxson

#pragma once
#include <cstddef>
#include <cstdint>
#include <memory>
#include <optional>
#include <string>
#include <vector>

// Bring in the global C ABI (all C structs/functions are global)
#include <idevice++/core_device_proxy.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/rsd.hpp>

namespace IdeviceFFI {

class DebugProxy {
  public:
    DebugProxy()                             = default;
    DebugProxy(const DebugProxy&)            = delete;
    DebugProxy& operator=(const DebugProxy&) = delete;

    DebugProxy(DebugProxy&& other) noexcept : handle_(other.handle_) { other.handle_ = nullptr; }
    DebugProxy& operator=(DebugProxy&& other) noexcept {
        if (this != &other) {
            reset();
            handle_       = other.handle_;
            other.handle_ = nullptr;
        }
        return *this;
    }

    ~DebugProxy() { reset(); }

    // Factory: connect over RSD (borrows adapter & handshake; does not consume them)
    static std::optional<DebugProxy>
    connect_rsd(Adapter& adapter, RsdHandshake& rsd, FfiError& err);

    // Factory: consume a ReadWrite stream (fat pointer)
    static std::optional<DebugProxy> from_readwrite_ptr(::ReadWriteOpaque* consumed, FfiError& err);

    // Convenience: consume a C++ ReadWrite wrapper by releasing it into the ABI
    static std::optional<DebugProxy> from_readwrite(ReadWrite&& rw, FfiError& err);

    // API
    std::optional<std::string>
    send_command(const std::string& name, const std::vector<std::string>& argv, FfiError& err);

    std::optional<std::string> read_response(FfiError& err);

    bool                       send_raw(const std::vector<uint8_t>& data, FfiError& err);

    // Reads up to `len` bytes; ABI returns a heap C string (we treat as bytes → string)
    std::optional<std::string> read(std::size_t len, FfiError& err);

    // Sets argv, returns textual reply (OK/echo/etc)
    std::optional<std::string> set_argv(const std::vector<std::string>& argv, FfiError& err);

    bool                       send_ack(FfiError& err);
    bool                       send_nack(FfiError& err);

    // No error object in ABI; immediate effect
    void set_ack_mode(bool enabled) { ::debug_proxy_set_ack_mode(handle_, enabled ? 1 : 0); }

    ::DebugProxyHandle* raw() const { return handle_; }

  private:
    explicit DebugProxy(::DebugProxyHandle* h) : handle_(h) {}

    void reset() {
        if (handle_) {
            ::debug_proxy_free(handle_);
            handle_ = nullptr;
        }
    }

    ::DebugProxyHandle* handle_ = nullptr;
};

// Small helper that owns a DebugserverCommandHandle
class DebugCommand {
  public:
    DebugCommand()                               = default;
    DebugCommand(const DebugCommand&)            = delete;
    DebugCommand& operator=(const DebugCommand&) = delete;

    DebugCommand(DebugCommand&& other) noexcept : handle_(other.handle_) {
        other.handle_ = nullptr;
    }
    DebugCommand& operator=(DebugCommand&& other) noexcept {
        if (this != &other) {
            reset();
            handle_       = other.handle_;
            other.handle_ = nullptr;
        }
        return *this;
    }

    ~DebugCommand() { reset(); }

    static std::optional<DebugCommand> make(const std::string&              name,
                                            const std::vector<std::string>& argv);

    ::DebugserverCommandHandle*        raw() const { return handle_; }

  private:
    explicit DebugCommand(::DebugserverCommandHandle* h) : handle_(h) {}

    void reset() {
        if (handle_) {
            ::debugserver_command_free(handle_);
            handle_ = nullptr;
        }
    }

    ::DebugserverCommandHandle* handle_ = nullptr;
};

} // namespace IdeviceFFI
