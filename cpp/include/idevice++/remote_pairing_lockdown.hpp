// Jackson Coxson

#pragma once
#include <cstdint>
#include <idevice++/bindings.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/idevice.hpp>
#include <idevice++/option.hpp>
#include <idevice++/provider.hpp>
#include <idevice++/rp_pairing_file.hpp>
#include <memory>
#include <string>
#include <vector>

namespace IdeviceFFI {

using RemotePairingLockdownPtr =
    std::unique_ptr<RemotePairingLockdownHandle,
                    FnDeleter<RemotePairingLockdownHandle, remote_pairing_lockdown_free>>;

// The `remotepairingdeviced` control channel, reached over USB through the
// `com.apple.dt.remotepairingdeviced.lockdown` lockdown service.
class RemotePairingLockdown {
  public:
    // Factory: connect via Provider. `sending_host` is the name this computer
    // identifies itself by, the same value the wireless flow uses.
    static Result<RemotePairingLockdown, FfiError> connect(Provider&          provider,
                                                           const std::string& sending_host);

    // Factory: wrap an existing lockdown connection to the service (consumes it)
    static Result<RemotePairingLockdown, FfiError> from_socket(Idevice&&          socket,
                                                               const std::string& sending_host);

    // Runs the control channel's handshake and returns what the device reports
    // about itself. The caller owns the returned plist.
    Result<plist_t, FfiError>                      attempt_pair_verify();

    // Checks whether the device still recognizes a pairing record. Run the
    // handshake first.
    Result<void, FfiError>                         validate_pairing(RpPairingFile& pairing_file);

    // Pairs with the device, updating `pairing_file` in place — write it out
    // afterwards to keep the pairing. Pairing over USB is promptless, so the
    // PIN should never be needed.
    Result<void, FfiError> pair(RpPairingFile& pairing_file, const Option<std::string>& pin = None);

    // Runs the handshake, validates `pairing_file`, and pairs only if that
    // fails.
    Result<void, FfiError> connect_pairing(RpPairingFile&             pairing_file,
                                           const Option<std::string>& pin = None);

    // The encryption key established during pairing, used as the TLS-PSK for
    // tunnel connections
    Result<std::vector<uint8_t>, FfiError> encryption_key() const;

    // RAII / moves
    ~RemotePairingLockdown() noexcept                                    = default;
    RemotePairingLockdown(RemotePairingLockdown&&) noexcept              = default;
    RemotePairingLockdown& operator=(RemotePairingLockdown&&) noexcept   = default;
    RemotePairingLockdown(const RemotePairingLockdown&)                  = delete;
    RemotePairingLockdown&       operator=(const RemotePairingLockdown&) = delete;

    RemotePairingLockdownHandle* raw() const noexcept { return handle_.get(); }
    static RemotePairingLockdown adopt(RemotePairingLockdownHandle* h) noexcept {
        return RemotePairingLockdown(h);
    }

  private:
    explicit RemotePairingLockdown(RemotePairingLockdownHandle* h) noexcept : handle_(h) {}
    RemotePairingLockdownPtr handle_{};
};

} // namespace IdeviceFFI
