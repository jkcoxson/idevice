// Jackson Coxson

#include <cstdlib>
#include <cstring>
#include <idevice++/bindings.hpp>
#include <idevice++/restore.hpp>

namespace IdeviceFFI {

namespace {

IdeviceFfiError* make_ffi_error(int32_t code, const char* msg) {
    auto* err     = static_cast<IdeviceFfiError*>(malloc(sizeof(IdeviceFfiError)));
    err->code     = code;
    err->sub_code = 0;
    err->message  = strdup(msg);
    return err;
}

/// Copies an FFI-owned byte buffer into a vector and frees the original.
std::vector<uint8_t> take_bytes(uint8_t* data, size_t len) {
    std::vector<uint8_t> out;
    if (data && len) {
        out.assign(data, data + len);
    }
    if (data) {
        ::idevice_data_free(data, len);
    }
    return out;
}

// ---- ComponentSource trampolines ------------------------------------------

extern "C" IdeviceFfiError*
component_read_trampoline(const char* path, uint8_t** out_data, size_t* out_len, void* ctx) {
    auto& cb  = *static_cast<ComponentSource*>(ctx);
    *out_data = nullptr;
    *out_len  = 0;
    if (!cb.read_component) {
        return make_ffi_error(-1, "no read_component callback");
    }
    try {
        auto data = cb.read_component(path ? path : "");
        *out_len  = data.size();
        if (!data.empty()) {
            *out_data = static_cast<uint8_t*>(malloc(data.size()));
            memcpy(*out_data, data.data(), data.size());
        }
        return nullptr;
    } catch (const std::exception& e) {
        return make_ffi_error(-1, e.what());
    } catch (...) {
        return make_ffi_error(-1, "read_component failed");
    }
}

// ---- FilesystemImage trampolines ------------------------------------------

extern "C" IdeviceFfiError* fs_size_trampoline(uint64_t* out_size, void* ctx) {
    auto& cb = *static_cast<FilesystemImage*>(ctx);
    if (!cb.size) {
        return make_ffi_error(-1, "no size callback");
    }
    try {
        *out_size = cb.size();
        return nullptr;
    } catch (const std::exception& e) {
        return make_ffi_error(-1, e.what());
    } catch (...) {
        return make_ffi_error(-1, "size failed");
    }
}

extern "C" IdeviceFfiError*
fs_read_at_trampoline(uint64_t offset, size_t len, uint8_t** out_data, size_t* out_len, void* ctx) {
    auto& cb  = *static_cast<FilesystemImage*>(ctx);
    *out_data = nullptr;
    *out_len  = 0;
    if (!cb.read_at) {
        return make_ffi_error(-1, "no read_at callback");
    }
    try {
        auto data = cb.read_at(offset, len);
        *out_len  = data.size();
        if (!data.empty()) {
            *out_data = static_cast<uint8_t*>(malloc(data.size()));
            memcpy(*out_data, data.data(), data.size());
        }
        return nullptr;
    } catch (const std::exception& e) {
        return make_ffi_error(-1, e.what());
    } catch (...) {
        return make_ffi_error(-1, "read_at failed");
    }
}

// ---- DataPortConnector trampoline -----------------------------------------

extern "C" IdeviceFfiError*
data_port_connect_trampoline(uint16_t port, IdeviceHandle** out_idevice, void* ctx) {
    auto& cb     = *static_cast<DataPortConnector*>(ctx);
    *out_idevice = nullptr;
    if (!cb.connect) {
        return make_ffi_error(-1, "no connect callback");
    }
    try {
        Idevice dev  = cb.connect(port);
        *out_idevice = dev.release();
        return nullptr;
    } catch (const std::exception& e) {
        return make_ffi_error(-1, e.what());
    } catch (...) {
        return make_ffi_error(-1, "data port connect failed");
    }
}

// ---- RestoreProgress trampolines ------------------------------------------

extern "C" void progress_operation_trampoline(uint64_t operation, uint64_t progress, void* ctx) {
    auto& cb = *static_cast<RestoreProgress*>(ctx);
    if (cb.operation) {
        cb.operation(operation, progress);
    }
}

extern "C" void progress_step_trampoline(const char* name, void* ctx) {
    auto& cb = *static_cast<RestoreProgress*>(ctx);
    if (cb.step) {
        cb.step(name ? name : "");
    }
}

extern "C" void progress_transfer_trampoline(
    const char* component, uint64_t sent, uint64_t total, bool has_total, void* ctx) {
    auto& cb = *static_cast<RestoreProgress*>(ctx);
    if (cb.transfer) {
        cb.transfer(component ? component : "", sent, has_total, total);
    }
}

} // anonymous namespace

// ===========================================================================
// Ipsw
// ===========================================================================

Result<Ipsw, FfiError> Ipsw::open(const std::string& path) {
    IpswHandle* handle = nullptr;
    FfiError    e(::idevice_ipsw_open(path.c_str(), &handle));
    if (e) {
        return Err(e);
    }
    return Ok(Ipsw::adopt(handle));
}

Result<plist_t, FfiError> Ipsw::build_manifest() {
    plist_t  manifest = nullptr;
    FfiError e(::idevice_ipsw_build_manifest(this->raw(), &manifest));
    if (e) {
        return Err(e);
    }
    return Ok(manifest);
}

Result<std::vector<uint8_t>, FfiError> Ipsw::read_component(plist_t            build_identity,
                                                            const std::string& name) {
    uint8_t* data = nullptr;
    size_t   len  = 0;
    FfiError e(
        ::idevice_ipsw_read_component(this->raw(), build_identity, name.c_str(), &data, &len));
    if (e) {
        return Err(e);
    }
    return Ok(take_bytes(data, len));
}

Result<std::vector<uint8_t>, FfiError> Ipsw::read_file(const std::string& path) {
    uint8_t* data = nullptr;
    size_t   len  = 0;
    FfiError e(::idevice_ipsw_read_file(this->raw(), path.c_str(), &data, &len));
    if (e) {
        return Err(e);
    }
    return Ok(take_bytes(data, len));
}

Result<void, FfiError> Ipsw::extract_to_file(const std::string& entry, const std::string& dest) {
    FfiError e(::idevice_ipsw_extract_to_file(this->raw(), entry.c_str(), dest.c_str()));
    if (e) {
        return Err(e);
    }
    return Ok();
}

// ===========================================================================
// RestoredClient
// ===========================================================================

Result<RestoredClient, FfiError> RestoredClient::connect(Idevice&& device) {
    RestoredClientHandle* handle = nullptr;
    FfiError              e(::idevice_restored_connect(device.release(), &handle));
    if (e) {
        return Err(e);
    }
    return Ok(RestoredClient::adopt(handle));
}

Result<RestoredClient, FfiError> RestoredClient::connect_by_ecid(const UsbmuxdAddr& addr,
                                                                 uint64_t           ecid,
                                                                 const std::string& label,
                                                                 uint64_t           timeout_ms) {
    RestoredClientHandle* handle = nullptr;
    FfiError              e(
        ::idevice_restored_connect_by_ecid(addr.raw(), ecid, label.c_str(), timeout_ms, &handle));
    if (e) {
        return Err(e);
    }
    return Ok(RestoredClient::adopt(handle));
}

Result<uint64_t, FfiError> RestoredClient::get_ecid() {
    uint64_t ecid = 0;
    FfiError e(::idevice_restored_get_ecid(this->raw(), &ecid));
    if (e) {
        return Err(e);
    }
    return Ok(ecid);
}

Option<uint32_t> RestoredClient::device_id() {
    uint32_t id            = 0;
    bool     has_device_id = false;
    FfiError e(::idevice_restored_get_device_id(this->raw(), &id, &has_device_id));
    if (e || !has_device_id) {
        return None;
    }
    return Some(id);
}

// ===========================================================================
// Standalone helpers
// ===========================================================================

Result<plist_t, FfiError> restore_options_new() {
    plist_t  options = nullptr;
    FfiError e(::idevice_restore_options_new(&options));
    if (e) {
        return Err(e);
    }
    return Ok(options);
}

Result<plist_t, FfiError> select_build_identity(plist_t             build_manifest,
                                                uint64_t            board_id,
                                                uint64_t            chip_id,
                                                Option<std::string> restore_behavior) {
    plist_t     identity = nullptr;
    const char* behavior = restore_behavior.is_some() ? restore_behavior.unwrap().c_str() : nullptr;
    FfiError    e(::idevice_restore_select_build_identity(
        build_manifest, board_id, chip_id, behavior, &identity));
    if (e) {
        return Err(e);
    }
    return Ok(identity);
}

Result<std::string, FfiError> component_path(plist_t build_identity, const std::string& name) {
    char*    path = nullptr;
    FfiError e(::idevice_restore_component_path(build_identity, name.c_str(), &path));
    if (e) {
        return Err(e);
    }
    std::string out = path ? path : "";
    if (path) {
        ::idevice_string_free(path);
    }
    return Ok(out);
}

bool has_component(plist_t build_identity, const std::string& name) {
    return component_path(build_identity, name).is_ok();
}

Result<std::vector<std::string>, FfiError> boot_component_names(plist_t build_identity) {
    char*    joined = nullptr;
    FfiError e(::idevice_restore_boot_component_names(build_identity, &joined));
    if (e) {
        return Err(e);
    }
    std::string s = joined ? joined : "";
    if (joined) {
        ::idevice_string_free(joined);
    }
    std::vector<std::string> out;
    size_t                   start = 0;
    while (start < s.size()) {
        size_t nl = s.find('\n', start);
        if (nl == std::string::npos) {
            out.push_back(s.substr(start));
            break;
        }
        out.push_back(s.substr(start, nl - start));
        start = nl + 1;
    }
    return Ok(out);
}

Result<Idevice, FfiError> connect_usb_port(const UsbmuxdAddr& addr,
                                           uint32_t           device_id,
                                           uint16_t           port,
                                           const std::string& label) {
    IdeviceHandle* handle = nullptr;
    FfiError       e(
        ::idevice_restore_connect_usb_port(addr.raw(), device_id, port, label.c_str(), &handle));
    if (e) {
        return Err(e);
    }
    return Ok(Idevice::adopt(handle));
}

Result<std::vector<uint8_t>, FfiError> fetch_ap_ticket(plist_t                     build_identity,
                                                       uint64_t                    board_id,
                                                       uint64_t                    chip_id,
                                                       uint64_t                    ecid,
                                                       const std::vector<uint8_t>& ap_nonce,
                                                       const std::vector<uint8_t>& sep_nonce) {
    uint8_t* ticket     = nullptr;
    size_t   ticket_len = 0;
    FfiError e(::idevice_restore_fetch_ap_ticket(build_identity,
                                                 board_id,
                                                 chip_id,
                                                 ecid,
                                                 ap_nonce.empty() ? nullptr : ap_nonce.data(),
                                                 ap_nonce.size(),
                                                 sep_nonce.empty() ? nullptr : sep_nonce.data(),
                                                 sep_nonce.size(),
                                                 &ticket,
                                                 &ticket_len));
    if (e) {
        return Err(e);
    }
    return Ok(take_bytes(ticket, ticket_len));
}

Result<std::vector<uint8_t>, FfiError> img4_stitch_component(const std::vector<uint8_t>& im4p,
                                                             const std::vector<uint8_t>& ticket,
                                                             const std::vector<uint8_t>& fourcc) {
    uint8_t*       out        = nullptr;
    size_t         out_len    = 0;
    const uint8_t* fourcc_ptr = fourcc.size() == 4 ? fourcc.data() : nullptr;
    FfiError       e(::idevice_img4_stitch_component(
        im4p.data(), im4p.size(), ticket.data(), ticket.size(), fourcc_ptr, &out, &out_len));
    if (e) {
        return Err(e);
    }
    return Ok(take_bytes(out, out_len));
}

Option<std::vector<uint8_t>> img4_restore_fourcc_override(const std::string& component_name) {
    uint8_t fourcc[4] = {0, 0, 0, 0};
    if (::idevice_img4_restore_fourcc_override(component_name.c_str(), fourcc)) {
        return Option<std::vector<uint8_t>>(std::vector<uint8_t>(fourcc, fourcc + 4));
    }
    return None;
}

// ===========================================================================
// Preboard stashbag
// ===========================================================================

Result<std::vector<uint8_t>, FfiError>
build_preboard_manifest(plist_t build_identity, uint64_t board_id, uint64_t chip_id) {
    uint8_t* data = nullptr;
    size_t   len  = 0;
    FfiError e(
        ::idevice_restore_build_preboard_manifest(build_identity, board_id, chip_id, &data, &len));
    if (e) {
        return Err(e);
    }
    return Ok(take_bytes(data, len));
}

// ===========================================================================
// CancelHandle
// ===========================================================================

CancelHandle::CancelHandle() : handle_(nullptr) {
    // A NULL out-handle is the only failure mode, so this cannot fail here.
    ::idevice_restore_cancel_handle_new(&handle_);
}

CancelHandle::~CancelHandle() {
    if (handle_) {
        ::idevice_restore_cancel_handle_free(handle_);
    }
}

CancelHandle::CancelHandle(CancelHandle&& other) noexcept : handle_(other.handle_) {
    other.handle_ = nullptr;
}

CancelHandle& CancelHandle::operator=(CancelHandle&& other) noexcept {
    if (this != &other) {
        if (handle_) {
            ::idevice_restore_cancel_handle_free(handle_);
        }
        handle_       = other.handle_;
        other.handle_ = nullptr;
    }
    return *this;
}

void CancelHandle::cancel() {
    if (handle_) {
        ::idevice_restore_cancel(handle_);
    }
}

// ===========================================================================
// run_restore
// ===========================================================================

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
                                   plist_t                     options) {
    IdeviceRestoreComponentSourceFFI comp_ffi{};
    comp_ffi.context         = &components;
    comp_ffi.read_component  = component_read_trampoline;
    comp_ffi.open_component  = nullptr;
    comp_ffi.read_chunk      = nullptr;
    comp_ffi.close_component = nullptr;

    IdeviceRestoreDataPortConnectorFFI ports_ffi{};
    ports_ffi.context = &data_ports;
    ports_ffi.connect = data_port_connect_trampoline;

    IdeviceRestoreFilesystemImageFFI  fs_ffi{};
    IdeviceRestoreFilesystemImageFFI* fs_ptr = nullptr;
    if (filesystem) {
        fs_ffi.context = filesystem;
        fs_ffi.size    = fs_size_trampoline;
        fs_ffi.read_at = fs_read_at_trampoline;
        fs_ptr         = &fs_ffi;
    }

    IdeviceRestoreProgressFFI  prog_ffi{};
    IdeviceRestoreProgressFFI* prog_ptr = nullptr;
    if (progress) {
        prog_ffi.context   = progress;
        prog_ffi.operation = progress->operation ? progress_operation_trampoline : nullptr;
        prog_ffi.step      = progress->step ? progress_step_trampoline : nullptr;
        prog_ffi.transfer  = progress->transfer ? progress_transfer_trampoline : nullptr;
        prog_ptr           = &prog_ffi;
    }

    FfiError e(::idevice_restore_run(client.raw(),
                                     build_identity,
                                     board_id,
                                     chip_id,
                                     ecid,
                                     tss_ticket.empty() ? nullptr : tss_ticket.data(),
                                     tss_ticket.size(),
                                     &comp_ffi,
                                     fs_ptr,
                                     &ports_ffi,
                                     prog_ptr,
                                     cancel ? cancel->raw() : nullptr,
                                     options));
    if (e) {
        return Err(e);
    }
    return Ok();
}

// ===========================================================================
// Recovery / DFU device
// ===========================================================================

namespace {

extern "C" IdeviceFfiError* rt_control_out(uint8_t        request_type,
                                           uint8_t        request,
                                           uint16_t       value,
                                           uint16_t       index,
                                           const uint8_t* data,
                                           size_t         data_len,
                                           uint32_t       timeout_ms,
                                           size_t*        out_transferred,
                                           void*          ctx) {
    auto& cb = *static_cast<RecoveryTransport*>(ctx);
    if (!cb.control_out) {
        return make_ffi_error(-1, "no control_out callback");
    }
    try {
        *out_transferred =
            cb.control_out(request_type, request, value, index, data, data_len, timeout_ms);
        return nullptr;
    } catch (const std::exception& e) {
        return make_ffi_error(-1, e.what());
    } catch (...) {
        return make_ffi_error(-1, "control_out failed");
    }
}

extern "C" IdeviceFfiError* rt_control_in(uint8_t   request_type,
                                          uint8_t   request,
                                          uint16_t  value,
                                          uint16_t  index,
                                          uint16_t  length,
                                          uint32_t  timeout_ms,
                                          uint8_t** out_data,
                                          size_t*   out_len,
                                          void*     ctx) {
    auto& cb  = *static_cast<RecoveryTransport*>(ctx);
    *out_data = nullptr;
    *out_len  = 0;
    if (!cb.control_in) {
        return make_ffi_error(-1, "no control_in callback");
    }
    try {
        auto data = cb.control_in(request_type, request, value, index, length, timeout_ms);
        *out_len  = data.size();
        if (!data.empty()) {
            *out_data = static_cast<uint8_t*>(malloc(data.size()));
            memcpy(*out_data, data.data(), data.size());
        }
        return nullptr;
    } catch (const std::exception& e) {
        return make_ffi_error(-1, e.what());
    } catch (...) {
        return make_ffi_error(-1, "control_in failed");
    }
}

extern "C" IdeviceFfiError* rt_bulk_out(uint8_t        endpoint,
                                        const uint8_t* data,
                                        size_t         data_len,
                                        uint32_t       timeout_ms,
                                        size_t*        out_transferred,
                                        void*          ctx) {
    auto& cb = *static_cast<RecoveryTransport*>(ctx);
    if (!cb.bulk_out) {
        return make_ffi_error(-1, "no bulk_out callback");
    }
    try {
        *out_transferred = cb.bulk_out(endpoint, data, data_len, timeout_ms);
        return nullptr;
    } catch (const std::exception& e) {
        return make_ffi_error(-1, e.what());
    } catch (...) {
        return make_ffi_error(-1, "bulk_out failed");
    }
}

extern "C" IdeviceFfiError* rt_serial_number(char* buf, size_t buf_len, void* ctx) {
    auto& cb = *static_cast<RecoveryTransport*>(ctx);
    if (!cb.serial_number) {
        return make_ffi_error(-1, "no serial_number callback");
    }
    try {
        std::string s = cb.serial_number();
        if (buf_len == 0) {
            return make_ffi_error(-1, "serial buffer too small");
        }
        size_t n = s.size() < buf_len - 1 ? s.size() : buf_len - 1;
        memcpy(buf, s.data(), n);
        buf[n] = '\0';
        return nullptr;
    } catch (const std::exception& e) {
        return make_ffi_error(-1, e.what());
    } catch (...) {
        return make_ffi_error(-1, "serial_number failed");
    }
}

extern "C" uint16_t rt_product_id(void* ctx) {
    auto& cb = *static_cast<RecoveryTransport*>(ctx);
    return cb.product_id ? cb.product_id() : 0;
}

extern "C" IdeviceFfiError* rt_set_configuration(uint8_t configuration, void* ctx) {
    auto& cb = *static_cast<RecoveryTransport*>(ctx);
    if (!cb.set_configuration) {
        return nullptr;
    }
    try {
        cb.set_configuration(configuration);
        return nullptr;
    } catch (const std::exception& e) {
        return make_ffi_error(-1, e.what());
    } catch (...) {
        return make_ffi_error(-1, "set_configuration failed");
    }
}

extern "C" IdeviceFfiError* rt_claim_interface(uint8_t interface, uint8_t alt_setting, void* ctx) {
    auto& cb = *static_cast<RecoveryTransport*>(ctx);
    if (!cb.claim_interface) {
        return nullptr;
    }
    try {
        cb.claim_interface(interface, alt_setting);
        return nullptr;
    } catch (const std::exception& e) {
        return make_ffi_error(-1, e.what());
    } catch (...) {
        return make_ffi_error(-1, "claim_interface failed");
    }
}

extern "C" IdeviceFfiError* rt_reset(void* ctx) {
    auto& cb = *static_cast<RecoveryTransport*>(ctx);
    if (!cb.reset) {
        return nullptr;
    }
    try {
        cb.reset();
        return nullptr;
    } catch (const std::exception& e) {
        return make_ffi_error(-1, e.what());
    } catch (...) {
        return make_ffi_error(-1, "reset failed");
    }
}

extern "C" IdeviceFfiError*
fdr_connect_trampoline(uint16_t port, IdeviceHandle** out_idevice, void* ctx) {
    auto& cb     = *static_cast<FdrConnector*>(ctx);
    *out_idevice = nullptr;
    if (!cb.connect_device_port) {
        return make_ffi_error(-1, "no connect_device_port callback");
    }
    try {
        Idevice dev  = cb.connect_device_port(port);
        *out_idevice = dev.release();
        return nullptr;
    } catch (const std::exception& e) {
        return make_ffi_error(-1, e.what());
    } catch (...) {
        return make_ffi_error(-1, "fdr connect failed");
    }
}

IdeviceRestoreRecoveryTransportFFI make_transport_ffi(RecoveryTransport& t) {
    IdeviceRestoreRecoveryTransportFFI d{};
    d.context           = &t;
    d.control_out       = rt_control_out;
    d.control_in        = rt_control_in;
    d.bulk_out          = rt_bulk_out;
    d.serial_number     = rt_serial_number;
    d.product_id        = rt_product_id;
    d.set_configuration = rt_set_configuration;
    d.claim_interface   = rt_claim_interface;
    d.reset             = rt_reset;
    return d;
}

} // anonymous namespace

Result<RecoveryDevice, FfiError> RecoveryDevice::open(RecoveryTransport& transport) {
    auto                  ffi    = make_transport_ffi(transport);
    RecoveryDeviceHandle* handle = nullptr;
    FfiError              e(::idevice_recovery_device_new(&ffi, &handle));
    if (e) {
        return Err(e);
    }
    return Ok(RecoveryDevice::adopt(handle));
}

Result<void, FfiError> RecoveryDevice::send_command(const std::string& command, uint8_t b_request) {
    FfiError e(::idevice_recovery_send_command(this->raw(), command.c_str(), b_request));
    if (e) {
        return Err(e);
    }
    return Ok();
}

Result<void, FfiError> RecoveryDevice::send_buffer(const uint8_t* data, size_t len) {
    FfiError e(::idevice_recovery_send_buffer(this->raw(), data, len));
    if (e) {
        return Err(e);
    }
    return Ok();
}

Result<std::vector<uint8_t>, FfiError> RecoveryDevice::getenv(const std::string& name) {
    uint8_t* data = nullptr;
    size_t   len  = 0;
    FfiError e(::idevice_recovery_getenv(this->raw(), name.c_str(), &data, &len));
    if (e) {
        return Err(e);
    }
    return Ok(take_bytes(data, len));
}

Result<void, FfiError> RecoveryDevice::setenv(const std::string& name, const std::string& value) {
    FfiError e(::idevice_recovery_setenv(this->raw(), name.c_str(), value.c_str()));
    if (e) {
        return Err(e);
    }
    return Ok();
}

Result<void, FfiError> RecoveryDevice::set_autoboot(bool enable) {
    FfiError e(::idevice_recovery_set_autoboot(this->raw(), enable));
    if (e) {
        return Err(e);
    }
    return Ok();
}

Result<void, FfiError> RecoveryDevice::finish_transfer() {
    FfiError e(::idevice_recovery_finish_transfer(this->raw()));
    if (e) {
        return Err(e);
    }
    return Ok();
}

Result<void, FfiError> RecoveryDevice::reboot() {
    FfiError e(::idevice_recovery_reboot(this->raw()));
    if (e) {
        return Err(e);
    }
    return Ok();
}

Result<uint16_t, FfiError> RecoveryDevice::product_id() {
    uint16_t pid         = 0;
    bool     is_recovery = false;
    FfiError e(::idevice_recovery_get_mode(this->raw(), &pid, &is_recovery));
    if (e) {
        return Err(e);
    }
    return Ok(pid);
}

Result<RecoveryInfo, FfiError> RecoveryDevice::info() {
    RecoveryInfo info;
    uint64_t     cpid = 0, bdid = 0, ecid = 0;
    bool         has_cpid = false, has_bdid = false, has_ecid = false;
    FfiError     e(::idevice_recovery_get_info(
        this->raw(), &cpid, &has_cpid, &bdid, &has_bdid, &ecid, &has_ecid));
    if (e) {
        return Err(e);
    }
    if (has_cpid) {
        info.cpid = cpid;
    }
    if (has_bdid) {
        info.bdid = bdid;
    }
    if (has_ecid) {
        info.ecid = ecid;
    }
    return Ok(info);
}

Option<std::vector<uint8_t>> RecoveryDevice::ap_nonce() {
    uint8_t* data = nullptr;
    size_t   len  = 0;
    if (::idevice_recovery_get_ap_nonce(this->raw(), &data, &len)) {
        return Option<std::vector<uint8_t>>(take_bytes(data, len));
    }
    return None;
}

Result<void, FfiError> fdr_start(FdrConnector& connector) {
    IdeviceRestoreFdrConnectorFFI ffi{};
    ffi.context             = &connector;
    ffi.connect_device_port = fdr_connect_trampoline;
    FfiError e(::idevice_restore_fdr_start(&ffi));
    if (e) {
        return Err(e);
    }
    return Ok();
}

} // namespace IdeviceFFI
