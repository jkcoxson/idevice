// Jackson Coxson
//
// A full C++ port of the `idevice-tools restore` flow, built on the idevice++
// restore wrapper.
//
// The recovery/DFU USB transport is supplied by the caller. On
// macOS and Linux this example implements it with libusb-1.0; on Windows (or
// when libusb is unavailable) the USB path is stubbed out and only the
// non-destructive preparation runs.

#include <chrono>
#include <csignal>
#include <cstdint>
#include <iostream>
#include <string>
#include <thread>
#include <vector>

#include <idevice++/idevice.hpp>
#include <idevice++/lockdown.hpp>
#include <idevice++/preboard_service.hpp>
#include <idevice++/provider.hpp>
#include <idevice++/restore.hpp>
#include <idevice++/usbmuxd.hpp>
#include <plist/plist++.h>

#if defined(IDEVICE_HAVE_LIBUSB)
#include <cstdio>
#include <fstream>
#include <libusb.h>
#include <memory>
#endif

using namespace IdeviceFFI;

namespace {

/// The in-flight restore's cancel handle, so a SIGINT handler can reach it.
/// A raw pointer suffices for a single-restore example; a real app should guard
/// its lifetime.
CancelHandle* g_cancel = nullptr;

void          print_usage(const char* prog) {
    std::cerr << "Usage: " << prog << " <ipsw> [--erase|--update] [--udid <udid>]\n"
              << "                 [--filesystem <dmg>] [--confirm]\n\n"
              << "  <ipsw>            Path to an IPSW (zip) file.\n"
              << "  --erase           Erase and restore (default: update).\n"
              << "  --update          Update, preserving user data (the default).\n"
              << "  --udid <id>       Target a specific device (default: first found).\n"
              << "  --filesystem <f>  Use an already-decrypted filesystem DMG.\n"
              << "  --confirm         Actually enter recovery and restore (DESTRUCTIVE).\n"
              << "                    Without it, only non-destructive preparation runs.\n";
}

uint64_t get_uint(Lockdown& ld, const char* key, uint64_t fallback) {
    auto r = ld.get_value(key, nullptr);
    if (r.is_err()) {
        return fallback;
    }
    try {
        PList::Integer node(r.unwrap());
        return node.GetValue();
    } catch (...) {
        return fallback;
    }
}

bool get_bool(Lockdown& ld, const char* key) {
    auto r = ld.get_value(key, nullptr);
    if (r.is_err()) {
        return false;
    }
    try {
        PList::Boolean node(r.unwrap());
        return node.GetValue();
    } catch (...) {
        return false;
    }
}

std::vector<uint8_t> get_data(Lockdown& ld, const char* key) {
    auto r = ld.get_value(key, nullptr);
    if (r.is_err()) {
        return {};
    }
    try {
        PList::Data       node(r.unwrap());
        std::vector<char> v = node.GetValue();
        return std::vector<uint8_t>(v.begin(), v.end());
    } catch (...) {
        return {};
    }
}

} // namespace

#if defined(IDEVICE_HAVE_LIBUSB)
namespace {

constexpr uint16_t APPLE_VID = 0x05AC;

// A usbmux data-port / FDR connector: connects to `port` on the device being
// restored (identified by `device_id`, from RestoredClient::device_id()). Throws on
// failure so the library can retry. The whole operation runs in one library call
// (splitting it corrupts tokio I/O state).
Idevice            connect_restore_usb_port(const UsbmuxdAddr& addr,
                                            uint32_t           device_id,
                                            uint16_t           port,
                                            const std::string& label) {
    auto dev = connect_usb_port(addr, device_id, port, label);
    if (dev.is_err()) {
        throw std::runtime_error("usbmux connect port " + std::to_string(port) + ": "
                                 + dev.unwrap_err().message);
    }
    return std::move(dev).unwrap();
}

RestoreProgress console_progress() {
    RestoreProgress p;
    p.operation = [](uint64_t op, uint64_t pct) {
        std::cout << "\r  operation " << op << ": " << pct << "%   " << std::flush;
        if (pct >= 100) {
            std::cout << std::endl;
        }
    };
    p.step     = [](const std::string& name) { std::cout << "\n> " << name << std::endl; };
    p.transfer = [](const std::string& comp, uint64_t sent, bool has_total, uint64_t total) {
        uint64_t mb = sent / (1024 * 1024);
        if (has_total && total) {
            std::cout << "\r    " << comp << ": " << mb << " MB (" << (sent * 100 / total)
                      << "%)   " << std::flush;
        } else {
            std::cout << "\r    " << comp << ": " << mb << " MB   " << std::flush;
        }
    };
    return p;
}

bool is_recovery_pid(uint16_t pid) {
    return (pid >= 0x1280 && pid <= 0x1283) || pid == 0x1222 || pid == 0x1227;
}

bool is_recovery_mode(uint16_t pid) {
    return pid >= 0x1280 && pid <= 0x1283; // recovery (iBoot), not DFU/WTF
}

// Parses the `ECID:<hex>` field out of a recovery serial-number string.
uint64_t ecid_from_serial(const std::string& serial) {
    auto pos = serial.find("ECID:");
    if (pos == std::string::npos) {
        return 0;
    }
    pos += 5;
    auto        end = serial.find(' ', pos);
    std::string hex = serial.substr(pos, end == std::string::npos ? std::string::npos : end - pos);
    try {
        return std::stoull(hex, nullptr, 16);
    } catch (...) {
        return 0;
    }
}

// An opened Apple recovery/DFU device.
struct UsbHandle {
    libusb_device_handle* h   = nullptr;
    uint16_t              pid = 0;
    std::string           serial;
    ~UsbHandle() {
        if (h) {
            libusb_close(h);
        }
    }
    UsbHandle()                            = default;
    UsbHandle(const UsbHandle&)            = delete;
    UsbHandle& operator=(const UsbHandle&) = delete;
};

// Scans USB for an Apple recovery/DFU device, optionally matching `ecid`, until
// `timeout` elapses. Returns an opened handle.
std::unique_ptr<UsbHandle>
find_recovery(libusb_context* ctx, bool match_ecid, uint64_t ecid, int timeout_s) {
    auto deadline = std::chrono::steady_clock::now() + std::chrono::seconds(timeout_s);
    for (;;) {
        libusb_device** list = nullptr;
        ssize_t         n    = libusb_get_device_list(ctx, &list);
        for (ssize_t i = 0; i < n; ++i) {
            libusb_device_descriptor desc{};
            if (libusb_get_device_descriptor(list[i], &desc) != 0) {
                continue;
            }
            if (desc.idVendor != APPLE_VID || !is_recovery_pid(desc.idProduct)) {
                continue;
            }
            libusb_device_handle* h = nullptr;
            if (libusb_open(list[i], &h) != 0) {
                continue;
            }
            std::string   serial;
            unsigned char buf[256];
            int r = libusb_get_string_descriptor_ascii(h, desc.iSerialNumber, buf, sizeof(buf));
            if (r > 0) {
                serial.assign(reinterpret_cast<char*>(buf), static_cast<size_t>(r));
            }
            if (match_ecid && ecid_from_serial(serial) != ecid) {
                libusb_close(h);
                continue;
            }
            libusb_free_device_list(list, 1);
            auto dev    = std::make_unique<UsbHandle>();
            dev->h      = h;
            dev->pid    = desc.idProduct;
            dev->serial = serial;
            return dev;
        }
        libusb_free_device_list(list, 1);
        if (std::chrono::steady_clock::now() >= deadline) {
            return nullptr;
        }
        std::this_thread::sleep_for(std::chrono::milliseconds(500));
    }
}

RecoveryTransport make_transport(std::unique_ptr<UsbHandle>& devref) {
    RecoveryTransport t;
    t.control_out = [&devref](uint8_t        rt,
                              uint8_t        req,
                              uint16_t       val,
                              uint16_t       idx,
                              const uint8_t* data,
                              size_t         len,
                              uint32_t       timeout) -> size_t {
        int r = libusb_control_transfer(devref->h,
                                        rt,
                                        req,
                                        val,
                                        idx,
                                        const_cast<unsigned char*>(data),
                                        static_cast<uint16_t>(len),
                                        timeout);
        if (r < 0) {
            throw std::runtime_error(std::string("control_out: ") + libusb_error_name(r));
        }
        return static_cast<size_t>(r);
    };
    t.control_in = [&devref](uint8_t  rt,
                             uint8_t  req,
                             uint16_t val,
                             uint16_t idx,
                             uint16_t length,
                             uint32_t timeout) -> std::vector<uint8_t> {
        std::vector<uint8_t> buf(length);
        int r = libusb_control_transfer(devref->h, rt, req, val, idx, buf.data(), length, timeout);
        if (r < 0) {
            throw std::runtime_error(std::string("control_in: ") + libusb_error_name(r));
        }
        buf.resize(static_cast<size_t>(r));
        return buf;
    };
    t.bulk_out =
        [&devref](uint8_t ep, const uint8_t* data, size_t len, uint32_t timeout) -> size_t {
        int transferred = 0;
        int r           = libusb_bulk_transfer(devref->h,
                                               ep,
                                               const_cast<unsigned char*>(data),
                                               static_cast<int>(len),
                                               &transferred,
                                               timeout);
        if (r < 0) {
            throw std::runtime_error(std::string("bulk_out: ") + libusb_error_name(r));
        }
        return static_cast<size_t>(transferred);
    };
    t.serial_number     = [&devref]() -> std::string { return devref->serial; };
    t.product_id        = [&devref]() -> uint16_t { return devref->pid; };
    t.set_configuration = [&devref](uint8_t cfg) {
        int current = -1;
        if (libusb_get_configuration(devref->h, &current) == 0
            && current == static_cast<int>(cfg)) {
            return;
        }
        int r = libusb_set_configuration(devref->h, cfg);
        if (r < 0 && r != LIBUSB_ERROR_BUSY) {
            std::fprintf(stderr, "warning: set_configuration(%u): %s\n", cfg, libusb_error_name(r));
        }
    };
    t.claim_interface = [&devref](uint8_t iface, uint8_t alt) {
        int r = libusb_claim_interface(devref->h, iface);
        if (r < 0) {
            std::fprintf(stderr, "claim_interface(%u): %s\n", iface, libusb_error_name(r));
            throw std::runtime_error(std::string("claim_interface: ") + libusb_error_name(r));
        }
        if (alt != 0) {
            r = libusb_set_interface_alt_setting(devref->h, iface, alt);
            if (r < 0) {
                std::fprintf(
                    stderr, "set_alt_setting(%u,%u): %s\n", iface, alt, libusb_error_name(r));
                throw std::runtime_error(std::string("set_alt_setting: ") + libusb_error_name(r));
            }
        }
    };
    t.reset = [&devref]() { libusb_reset_device(devref->h); }; // device re-enumerates
    return t;
}

// Reads a component from the IPSW, personalizes it, and uploads it to the device.
void send_component(RecoveryDevice&             rdev,
                    Ipsw&                       ipsw,
                    plist_t                     bi,
                    const std::vector<uint8_t>& ticket,
                    const std::string&          name) {
    auto raw = ipsw.read_component(bi, name);
    if (raw.is_err()) {
        throw std::runtime_error("read " + name + ": " + raw.unwrap_err().message);
    }
    std::vector<uint8_t> fourcc;
    auto                 ovr = img4_restore_fourcc_override(name);
    if (ovr.is_some()) {
        fourcc = ovr.unwrap();
    }
    auto img4 = img4_stitch_component(raw.unwrap(), ticket, fourcc);
    if (img4.is_err()) {
        throw std::runtime_error("stitch " + name + ": " + img4.unwrap_err().message);
    }
    auto data = img4.unwrap();
    std::cout << "  sending " << name << " (" << data.size() << " bytes)\n";
    auto sent = rdev.send_buffer(data.data(), data.size());
    if (sent.is_err()) {
        throw std::runtime_error("upload " + name + ": " + sent.unwrap_err().message);
    }
}

bool send_component_optional(RecoveryDevice&             rdev,
                             Ipsw&                       ipsw,
                             plist_t                     bi,
                             const std::vector<uint8_t>& ticket,
                             const std::string&          name) {
    try {
        send_component(rdev, ipsw, bi, ticket, name);
        return true;
    } catch (...) {
        return false;
    }
}

// Uploads the boot chain to place the device into restore mode. Mirrors the Rust
// CLI's boot_to_restore. Returns the (re-opened) recovery device.
RecoveryDevice boot_to_restore(libusb_context*             ctx,
                               std::unique_ptr<UsbHandle>& devref,
                               RecoveryTransport&          transport,
                               Ipsw&                       ipsw,
                               plist_t                     bi,
                               const std::vector<uint8_t>& ticket,
                               uint64_t                    ecid) {
    auto rdev_r = RecoveryDevice::open(transport);
    if (rdev_r.is_err()) {
        throw std::runtime_error("open recovery device: " + rdev_r.unwrap_err().message);
    }
    RecoveryDevice rdev = std::move(rdev_r).unwrap();

    if (is_recovery_mode(devref->pid)) {
        // Load iBEC and jump to it; the device re-enumerates.
        send_component(rdev, ipsw, bi, ticket, "iBEC");
        rdev.send_command("go", 1); // fire-and-forget
        rdev.finish_transfer();     // benign if it errors
        devref.reset();             // release the USB handle
        std::this_thread::sleep_for(std::chrono::seconds(3));
        std::cout << "waiting for iBEC to come up...\n";
        devref = find_recovery(ctx, true, ecid, 30);
        if (!devref) {
            throw std::runtime_error("iBEC device did not re-enumerate");
        }
        auto reopen = RecoveryDevice::open(transport);
        if (reopen.is_err()) {
            throw std::runtime_error("reopen after iBEC: " + reopen.unwrap_err().message);
        }
        rdev = std::move(reopen).unwrap();
    }

    const char* boot_args = "rd=md0 nand-enable-reformat=1 -progress";

    // Wait for iBEC to be ready (build-version becomes readable).
    for (int i = 0; i < 30; ++i) {
        auto v = rdev.getenv("build-version");
        if (v.is_ok() && !v.unwrap().empty()) {
            break;
        }
        std::this_thread::sleep_for(std::chrono::seconds(1));
    }

    rdev.set_autoboot(false).expect("set auto-boot false");

    // Apple logo (optional).
    if (send_component_optional(rdev, ipsw, bi, ticket, "RestoreLogo")) {
        rdev.send_command("setpicture 4");
        rdev.send_command("bgcolor 0 0 0");
    }

    // Components iBoot loads.
    auto boot_components = boot_component_names(bi).expect("boot component list");
    for (const auto& name : boot_components) {
        send_component(rdev, ipsw, bi, ticket, name);
        rdev.send_command("firmware").expect("firmware command");
    }

    send_component(rdev, ipsw, bi, ticket, "RestoreRamDisk");
    rdev.send_command("ramdisk").expect("ramdisk command");
    std::this_thread::sleep_for(std::chrono::seconds(2));

    send_component(rdev, ipsw, bi, ticket, "RestoreDeviceTree");
    rdev.send_command("devicetree").expect("devicetree command");

    if (has_component(bi, "RestoreSEP")
        && send_component_optional(rdev, ipsw, bi, ticket, "RestoreSEP")) {
        rdev.send_command("rsepfirmware");
    }

    send_component(rdev, ipsw, bi, ticket, "RestoreKernelCache");
    rdev.finish_transfer();
    rdev.send_command(std::string("setenv boot-args ") + boot_args).expect("setenv boot-args");
    rdev.send_command("bootx", 1);
    return rdev;
}

// A filesystem image backed by a DMG on disk.
struct FileImage {
    std::ifstream file;
    uint64_t      length = 0;
};

FilesystemImage make_file_image(FileImage& backing) {
    FilesystemImage fs;
    fs.size    = [&backing]() { return backing.length; };
    fs.read_at = [&backing](uint64_t offset, size_t len) {
        backing.file.clear();
        backing.file.seekg(static_cast<std::streamoff>(offset), std::ios::beg);
        std::vector<uint8_t> buf(len);
        backing.file.read(reinterpret_cast<char*>(buf.data()), static_cast<std::streamsize>(len));
        buf.resize(static_cast<size_t>(backing.file.gcount()));
        return buf;
    };
    return fs;
}

// Enters recovery, boots the ramdisk, and runs the restore to completion.
int run_full_restore(Lockdown&                   lockdown,
                     Ipsw&                       ipsw,
                     plist_t                     build_identity,
                     uint64_t                    board_id,
                     uint64_t                    chip_id,
                     uint64_t                    ecid,
                     const std::vector<uint8_t>& ticket,
                     const std::string&          filesystem_override,
                     const UsbmuxdAddr&          addr) {
    // Prepare the filesystem image up front (before the device drops lockdown).
    std::string dmg_path = filesystem_override;
    if (dmg_path.empty()) {
        auto os_path = component_path(build_identity, "OS");
        if (os_path.is_err()) {
            std::cerr << "build identity has no OS component; pass --filesystem\n";
            return 1;
        }
        dmg_path = "/tmp/idevice-restore-fs.dmg";
        std::cout << "extracting filesystem to " << dmg_path << " ...\n";
        auto ex = ipsw.extract_to_file(os_path.unwrap(), dmg_path);
        if (ex.is_err()) {
            std::cerr << "extract filesystem: " << ex.unwrap_err().message << "\n";
            return 1;
        }
    }
    FileImage fs_backing;
    fs_backing.file.open(dmg_path, std::ios::binary | std::ios::ate);
    if (!fs_backing.file) {
        std::cerr << "cannot open filesystem " << dmg_path << "\n";
        return 1;
    }
    fs_backing.length = static_cast<uint64_t>(fs_backing.file.tellg());

    // Enter recovery from normal mode.
    std::cout << "entering recovery...\n";
    lockdown.enter_recovery().expect("enter recovery");

    libusb_context* ctx = nullptr;
    if (libusb_init(&ctx) != 0) {
        std::cerr << "libusb_init failed\n";
        return 1;
    }
    std::unique_ptr<libusb_context, void (*)(libusb_context*)> ctx_guard(ctx, libusb_exit);

    std::cout << "scanning USB for the recovery device...\n";
    auto devref = find_recovery(ctx, true, ecid, 60);
    if (!devref) {
        std::cerr << "recovery device did not appear\n";
        return 1;
    }
    std::cout << "found recovery device (pid " << std::hex << devref->pid << std::dec
              << "), opening + booting restore ramdisk...\n";

    auto transport = make_transport(devref);
    auto rdev      = boot_to_restore(ctx, devref, transport, ipsw, build_identity, ticket, ecid);
    std::cout << "device instructed to boot into restore mode\n";
    (void) rdev; // the device now re-enumerates for restore mode over usbmux
    devref.reset();
    ctx_guard.reset();

    // Connect to restored on the re-enumerated device.
    auto     restored  = RestoredClient::connect_by_ecid(addr, ecid, "idevice-restore", 60000)
                             .expect("connect restored");
    // The usbmux id restored was found on; the data-port and FDR connectors reuse it
    // so their connections target this device rather than whichever USB device
    // usbmux lists first (which matters when more than one device is attached).
    uint32_t device_id = restored.device_id().expect("restored device id");
    std::cout << "connected to restored\n";

    // Start the FDR trust channel
    FdrConnector fdr;
    fdr.connect_device_port = [&addr, device_id](uint16_t port) {
        return connect_restore_usb_port(addr, device_id, port, "idevice-restore-fdr");
    };
    if (fdr_start(fdr).is_err()) {
        std::cerr << "warning: FDR did not start; continuing\n";
    }

    // Wire the state-machine delegates.
    ComponentSource comps;
    comps.read_component = [&ipsw](const std::string& path) {
        auto r = ipsw.read_file(path);
        if (r.is_err()) {
            throw std::runtime_error("read component " + path + ": " + r.unwrap_err().message);
        }
        return r.unwrap();
    };

    DataPortConnector ports;
    ports.connect = [&addr, device_id](uint16_t port) {
        return connect_restore_usb_port(addr, device_id, port, "idevice-restore-data");
    };

    FilesystemImage fs   = make_file_image(fs_backing);
    RestoreProgress prog = console_progress();
    auto            opts = restore_options_new().expect("restore options");

    // Ctrl-C requests a graceful cancel: the restore reboots the device toward
    // recovery instead of leaving it wedged.
    CancelHandle    cancel;
    g_cancel = &cancel;
    std::signal(SIGINT, [](int) {
        if (g_cancel) {
            g_cancel->cancel();
        }
    });

    std::cout << "running restore (press Ctrl-C to cancel)...\n";
    auto r   = restore_run(restored,
                           build_identity,
                           board_id,
                           chip_id,
                           ecid,
                           ticket,
                           comps,
                           &fs,
                           ports,
                           &prog,
                           &cancel,
                           opts);
    g_cancel = nullptr;
    if (r.is_err()) {
        std::cerr << "restore failed: " << r.unwrap_err().message << "\n";
        return 1;
    }
    std::cout << "restore complete\n";
    return 0;
}

} // namespace
#endif // IDEVICE_HAVE_LIBUSB

int main(int argc, char** argv) {
    std::string ipsw_path, udid_arg, filesystem_override;
    bool        erase = false, confirm = false;

    for (int i = 1; i < argc; ++i) {
        std::string a = argv[i];
        if (a == "--erase") {
            erase = true;
        } else if (a == "--update") {
            erase = false;
        } else if (a == "--confirm") {
            confirm = true;
        } else if (a == "--udid" && i + 1 < argc) {
            udid_arg = argv[++i];
        } else if (a == "--filesystem" && i + 1 < argc) {
            filesystem_override = argv[++i];
        } else if (ipsw_path.empty() && !a.empty() && a[0] != '-') {
            ipsw_path = a;
        } else {
            print_usage(argv[0]);
            return 1;
        }
    }
    if (ipsw_path.empty()) {
        print_usage(argv[0]);
        return 1;
    }
    const char* behavior = erase ? "Erase" : "Update";

    std::cout << std::unitbuf;

    try {
        // 1. Find a normal-mode device over usbmux
        auto u       = UsbmuxdConnection::default_new(0).expect("connect to usbmuxd");
        auto devices = u.get_devices().expect("list usbmux devices");
        if (devices.empty()) {
            std::cerr << "no devices connected\n";
            return 1;
        }
        UsbmuxdDevice* target = nullptr;
        if (!udid_arg.empty()) {
            for (auto& d : devices) {
                if (d.get_udid().unwrap_or("") == udid_arg) {
                    target = &d;
                    break;
                }
            }
            if (!target) {
                std::cerr << "device " << udid_arg << " not found\n";
                return 1;
            }
        } else {
            target = &devices[0];
        }
        auto udid = target->get_udid().expect("device udid");
        auto id   = target->get_id().expect("device id");

        // The usbmux address to reach the muxer on. Swap `default_new()` for
        // `unix_new(path)` / `tcp_new(...)` to target a non-default usbmuxd. It is
        // borrowed by the restore step (below), so keep it alive; the provider takes
        // its own owned copy.
        auto addr = UsbmuxdAddr::default_new();
        auto prov =
            Provider::usbmuxd_new(UsbmuxdAddr::default_new(), 0, udid, id, "idevice-restore")
                .expect("provider");

        // 2. Read every personalization identifier via lockdown
        auto lockdown = Lockdown::connect(prov).expect("lockdown connect");
        auto pairing  = prov.get_pairing_file().expect("pairing file");
        lockdown.start_session(pairing).expect("start session");

        uint64_t             board_id  = get_uint(lockdown, "BoardId", 0);
        uint64_t             chip_id   = get_uint(lockdown, "ChipID", 0);
        uint64_t             ecid      = get_uint(lockdown, "UniqueChipID", 0);
        bool                 has_sidp  = get_bool(lockdown, "HasSiDP");
        std::vector<uint8_t> ap_nonce  = get_data(lockdown, "ApNonce");
        std::vector<uint8_t> sep_nonce = get_data(lockdown, "SEPNonce");

        std::cout << "device: board=" << std::hex << board_id << " chip=" << chip_id
                  << " ecid=" << ecid << std::dec << "\n"
                  << "behavior: " << behavior << ", HasSiDP=" << (has_sidp ? "yes" : "no") << "\n";

        // 3. Open the IPSW and select the build identity
        auto ipsw           = Ipsw::open(ipsw_path).expect("open IPSW");
        auto build_manifest = ipsw.build_manifest().expect("read BuildManifest");
        auto build_identity =
            select_build_identity(build_manifest, board_id, chip_id, std::string(behavior))
                .expect("select build identity");
        std::cout << "selected " << behavior << " build identity\n";

        if (!confirm) {
            // Non-destructive preview: fetch the ticket and personalize a
            // component, then stop before touching the device.
            auto ticket =
                fetch_ap_ticket(build_identity, board_id, chip_id, ecid, ap_nonce, sep_nonce)
                    .expect("fetch AP ticket");
            std::cout << "got ApImg4Ticket (" << ticket.size() << " bytes)\n";
            auto kc = component_path(build_identity, "RestoreKernelCache");
            if (kc.is_ok()) {
                auto raw = ipsw.read_file(kc.unwrap());
                if (raw.is_ok()) {
                    std::vector<uint8_t> fourcc;
                    auto                 ovr = img4_restore_fourcc_override("RestoreKernelCache");
                    if (ovr.is_some()) {
                        fourcc = ovr.unwrap();
                    }
                    auto img4 = img4_stitch_component(raw.unwrap(), ticket, fourcc);
                    if (img4.is_ok()) {
                        std::cout << "personalized RestoreKernelCache -> " << img4.unwrap().size()
                                  << " byte IMG4\n";
                    }
                }
            }
            std::cout << "\nDry run complete. Re-run with --confirm to enter recovery and "
                         "restore the device (DESTRUCTIVE).\n";
            return 0;
        }

        // 4. Data-preserving update: create + commit a stashbag
        if (!erase && has_sidp) {
            std::cout << "update on a Secure-in-Data-Protection device; preparing stashbag...\n";
            auto manifest = build_preboard_manifest(build_identity, board_id, chip_id)
                                .expect("preboard manifest");
            auto preboard = PreboardService::connect(prov).expect("preboard connect");
            auto outcome  = preboard.create_stashbag(manifest).expect("create stashbag");
            if (outcome == StashbagOutcome::CommitRequired) {
                auto ticket =
                    fetch_ap_ticket(build_identity, board_id, chip_id, ecid, ap_nonce, sep_nonce)
                        .expect("fetch AP ticket (stashbag)");
                auto committer = PreboardService::connect(prov).expect("preboard reconnect");
                committer.commit_stashbag(ticket).expect("commit stashbag");
                std::cout << "stashbag committed; user data will be preserved\n";
            } else {
                std::cout << "device reported no stashbag required\n";
            }
        }

        // 5. Fetch the AP ticket and run the restore
        auto ticket = fetch_ap_ticket(build_identity, board_id, chip_id, ecid, ap_nonce, sep_nonce)
                          .expect("fetch AP ticket");
        std::cout << "got ApImg4Ticket (" << ticket.size() << " bytes)\n";

#if defined(IDEVICE_HAVE_LIBUSB)
        return run_full_restore(lockdown,
                                ipsw,
                                build_identity,
                                board_id,
                                chip_id,
                                ecid,
                                ticket,
                                filesystem_override,
                                addr);
#else
        (void) filesystem_override;
        (void) addr;
        std::cerr
            << "\nThis build has no USB support compiled in, so the recovery/DFU and\n"
               "restore steps cannot run. Rebuild with libusb-1.0 available (macOS/Linux)\n"
               "to drive a device end to end. Windows users can cry and choose a better OS.\n";
        return 1;
#endif
    } catch (const std::exception& e) {
        std::cerr << "error: " << e.what() << "\n";
        return 1;
    }
}
