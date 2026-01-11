// Jackson Coxson

#pragma once
#include <idevice++/bindings.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/provider.hpp>
#include <idevice++/result.hpp>
#include <string>
#include <vector>

namespace IdeviceFFI {

using DiagnosticsRelayPtr =
    std::unique_ptr<DiagnosticsRelayClientHandle,
                    FnDeleter<DiagnosticsRelayClientHandle, diagnostics_relay_client_free>>;

class DiagnosticsRelay {
  public:
    // Factory: connect via Provider
    static Result<DiagnosticsRelay, FfiError> connect(Provider& provider);

    // Factory: wrap an existing Idevice socket (consumes it on success)
    static Result<DiagnosticsRelay, FfiError> from_socket(Idevice&& socket);

    // API Methods - queries returning optional plist
    Result<Option<plist_t>, FfiError>         ioregistry(Option<std::string> current_plane,
                                                         Option<std::string> entry_name,
                                                         Option<std::string> entry_class) const;

    Result<Option<plist_t>, FfiError>         mobilegestalt(Option<std::vector<char*>> keys) const;

    Result<Option<plist_t>, FfiError>         gasguage() const;
    Result<Option<plist_t>, FfiError>         nand() const;
    Result<Option<plist_t>, FfiError>         all() const;
    Result<Option<plist_t>, FfiError>         wifi() const;

    // API Methods - actions
    Result<void, FfiError>                    restart();
    Result<void, FfiError>                    shutdown();
    Result<void, FfiError>                    sleep();
    Result<void, FfiError>                    goodbye();

    // RAII / moves
    ~DiagnosticsRelay() noexcept                                     = default;
    DiagnosticsRelay(DiagnosticsRelay&&) noexcept                    = default;
    DiagnosticsRelay& operator=(DiagnosticsRelay&&) noexcept         = default;
    DiagnosticsRelay(const DiagnosticsRelay&)                        = delete;
    DiagnosticsRelay&             operator=(const DiagnosticsRelay&) = delete;

    DiagnosticsRelayClientHandle* raw() const noexcept { return handle_.get(); }
    static DiagnosticsRelay       adopt(DiagnosticsRelayClientHandle* h) noexcept {
        return DiagnosticsRelay(h);
    }

  private:
    explicit DiagnosticsRelay(DiagnosticsRelayClientHandle* h) noexcept : handle_(h) {}
    DiagnosticsRelayPtr handle_{};
};

} // namespace IdeviceFFI