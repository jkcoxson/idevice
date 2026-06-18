// Jackson Coxson

#pragma once
#include <array>
#include <cstdint>
#include <idevice++/bindings.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/result.hpp>
#include <idevice++/rp_pairing_file.hpp>
#include <string>

namespace IdeviceFFI {

/// PIN display callback for device-initiated pairing.
/// Invoked with the 6-digit setup code the user must type into the device.
/// `pin` is only valid for the duration of the call.
using PinDisplayCallback = void (*)(const char* pin, void* context);

struct PairableHostResult {
    /// The resulting pairing file (host keys + paired device's altIRK).
    RpPairingFile pairing_file;
    /// The host's generated altIRK; persist this to re-advertise the host so an
    /// already-paired device recognizes it.
    std::array<uint8_t, 16> host_alt_irk;
};

/// Advertises this computer as a pairable host (`_remotepairing-pairable-host._tcp`)
/// and accepts a single device-initiated pairing (iOS 27+).
///
/// Blocks until a device connects and pairing completes or fails. `pin_callback`
/// is invoked once with the setup PIN to display to the user.
///
/// @param name        Name shown on the device.
/// @param model       Hardware model identifier shown on the device (e.g. "Mac17,7").
/// @param port        TCP port to listen on; 0 picks a free port.
/// @param pin_callback Called with the PIN to display. May be nullptr.
/// @param pin_context  Opaque pointer passed back to pin_callback.
Result<PairableHostResult, FfiError> accept_pairing(const std::string& name,
                                                    const std::string& model,
                                                    uint16_t           port         = 0,
                                                    PinDisplayCallback pin_callback = nullptr,
                                                    void*              pin_context  = nullptr);

} // namespace IdeviceFFI
