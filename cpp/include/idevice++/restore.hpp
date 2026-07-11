// Jackson Coxson
#pragma once

#include <cstdint>
#include <functional>
#include <idevice++/bindings.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/idevice.hpp>
#include <idevice++/option.hpp>
#include <idevice++/provider.hpp>
#include <idevice++/result.hpp>
#include <idevice++/usbmuxd.hpp>
#include <memory>
#include <string>
#include <vector>

namespace IdeviceFFI {

// ---------------------------------------------------------------------------
// Delegate callback bundles
//
// The Rust library is a protocol state machine that takes its I/O as traits;
// each bundle below is the C++ side of one of those traits. Populate the
// std::function members and hand the bundle to the matching call. A callback may
// throw to signal failure; the failure is surfaced back to the library.
// ---------------------------------------------------------------------------

/// Supplies firmware component bytes by their archive path.
struct ComponentSource {
    /// Reads the whole component at `path` into memory.
    std::function<std::vector<uint8_t>(const std::string& path)> read_component;
};

/// A seekable, sized filesystem image (the OS DMG) for ASR.
struct FilesystemImage {
    /// Total image size in bytes.
    std::function<uint64_t()>                                        size;
    /// Reads up to `len` bytes starting at `offset`.
    std::function<std::vector<uint8_t>(uint64_t offset, size_t len)> read_at;
};

/// Opens fresh connections to restore-mode data ports.
struct DataPortConnector {
    /// Connects to `port`, returning a new Idevice connection.
    std::function<Idevice(uint16_t port)> connect;
};

/// Progress callbacks for driving a UI. Any member may be left empty.
struct RestoreProgress {
    /// The device's operation code and completion percentage (0–100).
    std::function<void(uint64_t operation, uint64_t progress)> operation;
    /// The host has begun a named step (the `DataType` being serviced).
    std::function<void(const std::string& name)>               step;
    /// Byte progress while streaming a large image; `has_total`=false ⇒ unknown.
    std::function<void(const std::string& component, uint64_t sent, bool has_total, uint64_t total)>
        transfer;
};

/// The raw USB surface of a device in recovery/DFU mode. A callback may throw to
/// signal a transport error.
struct RecoveryTransport {
    std::function<size_t(uint8_t        request_type,
                         uint8_t        request,
                         uint16_t       value,
                         uint16_t       index,
                         const uint8_t* data,
                         size_t         data_len,
                         uint32_t       timeout_ms)>
        control_out;
    std::function<std::vector<uint8_t>(uint8_t  request_type,
                                       uint8_t  request,
                                       uint16_t value,
                                       uint16_t index,
                                       uint16_t length,
                                       uint32_t timeout_ms)>
        control_in;
    std::function<size_t(
        uint8_t endpoint, const uint8_t* data, size_t data_len, uint32_t timeout_ms)>
                                                                bulk_out;
    std::function<std::string()>                                serial_number;
    std::function<uint16_t()>                                   product_id;
    std::function<void(uint8_t configuration)>                  set_configuration;
    std::function<void(uint8_t interface, uint8_t alt_setting)> claim_interface;
    std::function<void()>                                       reset;
};

/// Opens FDR trust-channel connections to device ports.
struct FdrConnector {
    std::function<Idevice(uint16_t port)> connect_device_port;
};

// ---------------------------------------------------------------------------
// IPSW archive
// ---------------------------------------------------------------------------

using IpswPtr = std::unique_ptr<IpswHandle, FnDeleter<IpswHandle, idevice_ipsw_free>>;

class Ipsw {
  public:
    static Result<Ipsw, FfiError>          open(const std::string& path);

    Result<plist_t, FfiError>              build_manifest();
    Result<std::vector<uint8_t>, FfiError> read_component(plist_t            build_identity,
                                                          const std::string& name);
    Result<std::vector<uint8_t>, FfiError> read_file(const std::string& path);
    Result<void, FfiError> extract_to_file(const std::string& entry, const std::string& dest);

    IpswHandle*            raw() const noexcept { return handle_.get(); }
    static Ipsw            adopt(IpswHandle* h) noexcept { return Ipsw(h); }

  private:
    explicit Ipsw(IpswHandle* h) noexcept : handle_(h) {}
    IpswPtr handle_{};
};

// ---------------------------------------------------------------------------
// restored client
// ---------------------------------------------------------------------------

using RestoredClientPtr =
    std::unique_ptr<RestoredClientHandle, FnDeleter<RestoredClientHandle, idevice_restored_free>>;

class RestoredClient {
  public:
    /// Connects to `com.apple.mobile.restored` over an existing Idevice (consumes it).
    static Result<RestoredClient, FfiError> connect(Idevice&& device);

    /// Finds a restore-mode device by ECID over usbmux and connects to it.
    /// `addr` is borrowed (the caller keeps ownership).
    static Result<RestoredClient, FfiError> connect_by_ecid(const UsbmuxdAddr& addr,
                                                            uint64_t           ecid,
                                                            const std::string& label,
                                                            uint64_t           timeout_ms);

    Result<uint64_t, FfiError>              get_ecid();

    /// The usbmux `device_id` this client was found on, when discovered via
    /// `connect_by_ecid` (else `None`). Pass it to `connect_usb_port` so data-port
    /// and FDR connections target the same device being restored.
    Option<uint32_t>                        device_id();

    RestoredClientHandle*                   raw() const noexcept { return handle_.get(); }
    static RestoredClient adopt(RestoredClientHandle* h) noexcept { return RestoredClient(h); }

  private:
    explicit RestoredClient(RestoredClientHandle* h) noexcept : handle_(h) {}
    RestoredClientPtr handle_{};
};

// ---------------------------------------------------------------------------
// Standalone helpers (personalization + build identity selection + options)
// ---------------------------------------------------------------------------

/// Builds the default iOS `RestoreOptions` plist (caller frees with plist_free).
Result<plist_t, FfiError>     restore_options_new();

/// Selects the `BuildIdentity` matching the device from a `BuildManifest` plist.
Result<plist_t, FfiError>     select_build_identity(plist_t             build_manifest,
                                                    uint64_t            board_id,
                                                    uint64_t            chip_id,
                                                    Option<std::string> restore_behavior);

/// Resolves the archive path of a component within a build identity.
Result<std::string, FfiError> component_path(plist_t build_identity, const std::string& name);

/// Whether a build identity contains a component (by name).
bool                          has_component(plist_t build_identity, const std::string& name);

/// The components iBoot loads during the restore boot, in manifest order.
Result<std::vector<std::string>, FfiError> boot_component_names(plist_t build_identity);

/// Connects to `port` on the usbmux device identified by `device_id` - a
/// ready-made building block for `DataPortConnector` / `FdrConnector` callbacks.
/// `addr` is borrowed (the caller keeps ownership). `device_id` must come from
/// `RestoredClient::device_id()` so the connection is pinned to the device being
/// restored.
Result<Idevice, FfiError>                  connect_usb_port(const UsbmuxdAddr& addr,
                                                            uint32_t           device_id,
                                                            uint16_t           port,
                                                            const std::string& label);

/// Fetches the AP `ApImg4Ticket` (IM4M) from Apple's TSS server.
Result<std::vector<uint8_t>, FfiError>     fetch_ap_ticket(plist_t                     build_identity,
                                                           uint64_t                    board_id,
                                                           uint64_t                    chip_id,
                                                           uint64_t                    ecid,
                                                           const std::vector<uint8_t>& ap_nonce,
                                                           const std::vector<uint8_t>& sep_nonce);

/// Stitches an `IM4P` component with a ticket into a personalized `IMG4`.
/// `fourcc`, if non-empty, must be exactly 4 bytes.
Result<std::vector<uint8_t>, FfiError> img4_stitch_component(const std::vector<uint8_t>& im4p,
                                                             const std::vector<uint8_t>& ticket,
                                                             const std::vector<uint8_t>& fourcc);

/// Returns the re-tag fourcc a `Restore*` component needs, if any.
Option<std::vector<uint8_t>> img4_restore_fourcc_override(const std::string& component_name);

// ---------------------------------------------------------------------------
// Preboard stashbag (data-preserving updates)
// ---------------------------------------------------------------------------
// The stashbag client itself is `PreboardService` (idevice++/preboard_service.hpp);
// it is only relevant to data-preserving *update* restores of `HasSiDP` devices.

/// Builds the local (unsigned) `IM4M` preboard manifest for a stashbag request.
Result<std::vector<uint8_t>, FfiError>
build_preboard_manifest(plist_t build_identity, uint64_t board_id, uint64_t chip_id);

// ---------------------------------------------------------------------------
// run_restore
// ---------------------------------------------------------------------------

/// A shareable cancellation flag for an in-flight restore.
///
/// Construct one, pass its address to `restore_run`, and call `cancel()` from
/// another thread to request a graceful cancel: the restore stops and the device
/// is rebooted toward recovery (so it can be retried rather than left wedged).
/// Move-only; the underlying handle is freed on destruction.
class CancelHandle {
  public:
    CancelHandle();
    ~CancelHandle();
    CancelHandle(CancelHandle&& other) noexcept;
    CancelHandle& operator=(CancelHandle&& other) noexcept;
    CancelHandle(const CancelHandle&)                          = delete;
    CancelHandle&               operator=(const CancelHandle&) = delete;

    /// Requests cancellation. Thread-safe; may be called while a restore runs.
    void                        cancel();

    /// The underlying FFI handle, for `restore_run`.
    IdeviceRestoreCancelHandle* raw() const { return handle_; }

  private:
    IdeviceRestoreCancelHandle* handle_;
};

/// Drives the restore-mode state machine to completion.
///
/// `filesystem`, `progress`, and `cancel` are optional (pass nullptr). `options`
/// may be the plist from `restore_options_new` (or nullptr for defaults). When
/// `cancel` is provided and triggered from another thread, this returns a restore
/// `Cancelled` error after rebooting the device toward recovery.
Result<void, FfiError> restore_run(RestoredClient&             client,
                                   plist_t                     build_identity,
                                   uint64_t                    board_id,
                                   uint64_t                    chip_id,
                                   uint64_t                    ecid,
                                   const std::vector<uint8_t>& tss_ticket,
                                   ComponentSource&            components,
                                   FilesystemImage*            filesystem,
                                   DataPortConnector&          data_ports,
                                   RestoreProgress*            progress,
                                   CancelHandle*               cancel,
                                   plist_t                     options);

// ---------------------------------------------------------------------------
// Recovery / DFU device
// ---------------------------------------------------------------------------

using RecoveryDevicePtr =
    std::unique_ptr<RecoveryDeviceHandle,
                    FnDeleter<RecoveryDeviceHandle, idevice_recovery_device_free>>;

/// Identifiers parsed from a recovery/DFU device's serial string.
struct RecoveryInfo {
    Option<uint64_t> cpid;
    Option<uint64_t> bdid;
    Option<uint64_t> ecid;
};

class RecoveryDevice {
  public:
    /// Opens a recovery/DFU device over a caller-supplied transport.
    static Result<RecoveryDevice, FfiError> open(RecoveryTransport& transport);

    Result<void, FfiError> send_command(const std::string& command, uint8_t b_request = 0);
    Result<void, FfiError> send_buffer(const uint8_t* data, size_t len);
    Result<std::vector<uint8_t>, FfiError> getenv(const std::string& name);
    Result<void, FfiError>         setenv(const std::string& name, const std::string& value);
    Result<void, FfiError>         set_autoboot(bool enable);
    Result<void, FfiError>         finish_transfer();
    Result<void, FfiError>         reboot();

    Result<uint16_t, FfiError>     product_id();
    Result<RecoveryInfo, FfiError> info();
    Option<std::vector<uint8_t>>   ap_nonce();

    RecoveryDeviceHandle*          raw() const noexcept { return handle_.get(); }
    static RecoveryDevice adopt(RecoveryDeviceHandle* h) noexcept { return RecoveryDevice(h); }

  private:
    explicit RecoveryDevice(RecoveryDeviceHandle* h) noexcept : handle_(h) {}
    RecoveryDevicePtr handle_{};
};

/// Starts the FDR trust channel (control handshake + background listener).
Result<void, FfiError> fdr_start(FdrConnector& connector);

} // namespace IdeviceFFI
