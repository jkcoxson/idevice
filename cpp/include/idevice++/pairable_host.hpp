// Jackson Coxson

#pragma once
#include <array>
#include <cstdint>
#include <idevice++/bindings.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/idevice.hpp>
#include <idevice++/option.hpp>
#include <idevice++/result.hpp>
#include <idevice++/rp_pairing_file.hpp>
#include <idevice++/tunnel_provider.hpp>
#include <memory>
#include <string>

namespace IdeviceFFI {

/// PIN display callback for device-initiated pairing.
/// Invoked with the 6-digit setup code the user must type into the device.
/// `pin` is only valid for the duration of the call.
using PinDisplayCallback = void (*)(const char* pin, void* context);

using PairableHostCancelPtr =
    std::unique_ptr<::PairableHostCancel,
                    FnDeleter<::PairableHostCancel, pairable_host_cancel_free>>;

/// Cancellation token for accept_pairing().
///
/// Create one before calling accept_pairing(), hand it to the call, and signal()
/// it from another thread to abort the wait (e.g. the user dismissed the pairing
/// UI). Without one, the accept can only be ended by a device connecting.
class PairableHostCancelToken {
  public:
    /// Allocates a new token. Returns None only if allocation fails.
    static Option<PairableHostCancelToken> create() noexcept;

    /// Unblocks the accept this token was passed to; it returns CanceledByUser.
    /// Safe to call from any thread, before or during the accept, and more than
    /// once. Signalling a token that was never used, or whose accept already
    /// returned, does nothing.
    void                                   signal() const noexcept;

    // RAII / moves
    ~PairableHostCancelToken() noexcept                                    = default;
    PairableHostCancelToken(PairableHostCancelToken&&) noexcept            = default;
    PairableHostCancelToken& operator=(PairableHostCancelToken&&) noexcept = default;
    PairableHostCancelToken(const PairableHostCancelToken&)                = delete;
    PairableHostCancelToken& operator=(const PairableHostCancelToken&)     = delete;

    ::PairableHostCancel*    raw() const noexcept { return handle_.get(); }

  private:
    explicit PairableHostCancelToken(::PairableHostCancel* h) noexcept : handle_(h) {}
    PairableHostCancelPtr handle_{};
};

struct PairableHostResult {
    /// The resulting pairing file (host keys + paired device's altIRK).
    RpPairingFile           pairing_file;
    /// The host's generated altIRK; persist this to re-advertise the host so an
    /// already-paired device recognizes it.
    std::array<uint8_t, 16> host_alt_irk;
    /// The paired device's identity (name, model, UDID, altIRK), when the device
    /// supplied one.
    Option<PeerDeviceInfo>  peer_device;
};

/// Advertises this computer as a pairable host (`_remotepairing-pairable-host._tcp`)
/// and accepts a single device-initiated pairing (iOS 27+).
///
/// Blocks until a device connects and pairing completes or fails, or until `cancel`
/// is signalled from another thread. `pin_callback` is invoked once with the setup
/// PIN to display to the user.
///
/// @param name        Name shown on the device.
/// @param model       Hardware model identifier shown on the device (e.g. "Mac17,7").
/// @param port        TCP port to listen on; 0 picks a free port.
/// @param pin_callback Called with the PIN to display. May be nullptr.
/// @param pin_context  Opaque pointer passed back to pin_callback.
/// @param cancel       Token to abort the wait from another thread. May be nullptr,
///                     in which case only a connecting device ends the call.
Result<PairableHostResult, FfiError>
accept_pairing(const std::string&             name,
               const std::string&             model,
               uint16_t                       port         = 0,
               PinDisplayCallback             pin_callback = nullptr,
               void*                          pin_context  = nullptr,
               const PairableHostCancelToken* cancel       = nullptr);

} // namespace IdeviceFFI
