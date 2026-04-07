// Jackson Coxson
// dvt_demo.cpp — exercises DeviceInfo, ApplicationListing, ConditionInducer,
//                NetworkMonitor, and Sysmontap over an RSD/CoreDeviceProxy connection.
//
// Usage:
//   dvt_demo
//   dvt_demo <udid>

#include <cstdlib>
#include <iostream>
#include <string>
#include <vector>

#include <idevice++/bindings.hpp>
#include <idevice++/core_device_proxy.hpp>
#include <idevice++/dvt/application_listing.hpp>
#include <idevice++/dvt/condition_inducer.hpp>
#include <idevice++/dvt/device_info.hpp>
#include <idevice++/dvt/network_monitor.hpp>
#include <idevice++/dvt/remote_server.hpp>
#include <idevice++/dvt/sysmontap.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/provider.hpp>
#include <idevice++/rsd.hpp>
#include <idevice++/usbmuxd.hpp>

using namespace IdeviceFFI;

[[noreturn]] static void die(const char* msg, const FfiError& e) {
    std::cerr << "[ERROR] " << msg << ": " << e.message << "\n";
    std::exit(1);
}

// ─── DeviceInfo demo ─────────────────────────────────────────────────────────

static void demo_device_info(RemoteServer& rs) {
    std::cout << "\n═══════════════════════════════════════\n";
    std::cout << " DeviceInfo\n";
    std::cout << "═══════════════════════════════════════\n";

    auto di = DeviceInfo::create(rs).expect("DeviceInfo::create");

    auto kernel = di.mach_kernel_name().expect("mach_kernel_name");
    std::cout << "Kernel: " << kernel << "\n";

    auto procs = di.running_processes().expect("running_processes");
    std::cout << "Running processes: " << procs.size() << " total (first 10):\n";
    for (size_t i = 0; i < procs.size() && i < 10; ++i) {
        auto& p = procs[i];
        std::cout << "  [" << p.pid << "] " << p.name;
        if (p.is_application) std::cout << " (app)";
        std::cout << "\n";
    }

    auto proc_attrs = di.sysmon_process_attributes().expect("sysmon_process_attributes");
    auto sys_attrs  = di.sysmon_system_attributes().expect("sysmon_system_attributes");
    std::cout << "Sysmon: " << proc_attrs.size() << " process attrs, "
              << sys_attrs.size() << " system attrs\n";

    auto entries = di.directory_listing("/").expect("directory_listing");
    std::cout << "/ contents (" << entries.size() << " entries):";
    for (auto& e : entries) std::cout << " " << e;
    std::cout << "\n";

    auto hw = di.hardware_information().expect("hardware_information");
    if (hw) { std::cout << "Hardware info: received plist dict\n"; plist_free(hw); }

    auto net = di.network_information().expect("network_information");
    if (net) { std::cout << "Network info: received plist dict\n"; plist_free(net); }
}

// ─── ApplicationListing demo ─────────────────────────────────────────────────

static void demo_application_listing(RemoteServer& rs) {
    std::cout << "\n═══════════════════════════════════════\n";
    std::cout << " ApplicationListing\n";
    std::cout << "═══════════════════════════════════════\n";

    auto al   = ApplicationListing::create(rs).expect("ApplicationListing::create");
    auto apps = al.installed_applications().expect("installed_applications");

    std::cout << "Installed applications: " << apps.size() << "\n";

    // Print first 5 display names
    size_t shown = 0;
    for (plist_t app : apps) {
        if (shown < 5 && app != nullptr) {
            plist_t name_node = plist_dict_get_item(app, "DisplayName");
            if (name_node != nullptr) {
                char* str = nullptr;
                plist_get_string_val(name_node, &str);
                if (str) {
                    std::cout << "  " << str << "\n";
                    free(str);
                    ++shown;
                }
            }
        }
        plist_free(app);
    }
    if (apps.size() > 5) {
        std::cout << "  ... and " << (apps.size() - 5) << " more\n";
    }
}

// ─── ConditionInducer demo ────────────────────────────────────────────────────

static void demo_condition_inducer(RemoteServer& rs) {
    std::cout << "\n═══════════════════════════════════════\n";
    std::cout << " ConditionInducer\n";
    std::cout << "═══════════════════════════════════════\n";

    auto ci     = ConditionInducer::create(rs).expect("ConditionInducer::create");
    auto groups = ci.available_conditions().expect("available_conditions");

    std::cout << "Condition groups: " << groups.size() << "\n";
    for (auto& g : groups) {
        std::cout << "  [" << g.identifier << "] " << g.profiles.size() << " profiles\n";
        for (auto& p : g.profiles) {
            std::cout << "    • " << p.identifier << ": " << p.description << "\n";
        }
    }
}

// ─── NetworkMonitor demo ──────────────────────────────────────────────────────

static void demo_network_monitor(RemoteServer& rs) {
    std::cout << "\n═══════════════════════════════════════\n";
    std::cout << " NetworkMonitor (first 5 events)\n";
    std::cout << "═══════════════════════════════════════\n";

    auto nm = NetworkMonitor::create(rs).expect("NetworkMonitor::create");
    nm.start().expect("start");

    for (int i = 0; i < 5; ++i) {
        auto ev = nm.next_event().expect("next_event");
        if (ev.event_type == InterfaceDetection) {
            std::cout << "  [interface] idx=" << ev.interface_index
                      << " name=" << ev.interface_name << "\n";
        } else if (ev.event_type == ConnectionDetection) {
            std::cout << "  [new_conn]  pid=" << ev.pid << "  "
                      << ev.local_addr.addr << ":" << ev.local_addr.port
                      << " -> "
                      << ev.remote_addr.addr << ":" << ev.remote_addr.port << "\n";
        } else if (ev.event_type == ConnectionUpdate) {
            std::cout << "  [conn_upd]  serial=" << ev.connection_serial
                      << "  rx=" << ev.rx_bytes << "B tx=" << ev.tx_bytes << "B\n";
        } else {
            std::cout << "  [unknown type=" << ev.unknown_type << "]\n";
        }
    }

    nm.stop().expect("stop");
}

// ─── Sysmontap demo ───────────────────────────────────────────────────────────

static void demo_sysmontap(RemoteServer& rs) {
    std::cout << "\n═══════════════════════════════════════\n";
    std::cout << " Sysmontap (3 samples at 500ms)\n";
    std::cout << "═══════════════════════════════════════\n";

    auto di         = DeviceInfo::create(rs).expect("DeviceInfo for sysmontap");
    auto proc_attrs = di.sysmon_process_attributes().expect("proc attrs");
    auto sys_attrs  = di.sysmon_system_attributes().expect("sys attrs");

    auto sm = Sysmontap::create(rs).expect("Sysmontap::create");
    sm.set_config(500, proc_attrs, sys_attrs).expect("set_config");
    sm.start().expect("start");

    for (int i = 0; i < 3; ++i) {
        auto s = sm.next_sample().expect("next_sample");
        std::cout << "  Sample " << (i + 1) << ":";

        if (s.system_cpu_usage != nullptr) {
            plist_t load_node = plist_dict_get_item(s.system_cpu_usage, "CPU_TotalLoad");
            if (load_node != nullptr) {
                double load = 0.0;
                plist_get_real_val(load_node, &load);
                std::cout << "  cpu=" << load << "%";
            }
            plist_free(s.system_cpu_usage);
        }
        if (s.processes != nullptr) {
            std::cout << "  procs=" << plist_dict_get_size(s.processes);
            plist_free(s.processes);
        }
        if (s.system != nullptr) {
            std::cout << "  sys_vals=" << plist_array_get_size(s.system);
            plist_free(s.system);
        }
        std::cout << "\n";
    }

    sm.stop().expect("stop");
}

// ─── main ─────────────────────────────────────────────────────────────────────

int main(int argc, char** argv) {
    idevice_init_logger(Warn, Disabled, nullptr);

    // Connect to usbmuxd and pick a device
    auto mux     = UsbmuxdConnection::default_new(0).expect("usbmuxd connect");
    auto devices = mux.get_devices().expect("get_devices");
    if (devices.empty()) {
        std::cerr << "No devices connected.\n";
        return 1;
    }

    int chosen = 0;
    if (argc >= 2) {
        std::string want = argv[1];
        for (int i = 0; i < static_cast<int>(devices.size()); ++i) {
            auto u = devices[i].get_udid();
            if (u.is_some() && u.unwrap() == want) { chosen = i; break; }
        }
    }

    auto& dev    = devices[chosen];
    auto  udid   = dev.get_udid();
    auto  mux_id = dev.get_id();
    if (udid.is_none() || mux_id.is_none()) {
        std::cerr << "Device has no UDID or mux-id.\n";
        return 1;
    }
    std::cout << "Device: " << udid.unwrap() << "\n";

    auto addr     = UsbmuxdAddr::default_new();
    auto provider = Provider::usbmuxd_new(std::move(addr), 0,
                                          udid.unwrap(), mux_id.unwrap(),
                                          "dvt-demo")
                        .expect("Provider::usbmuxd_new");

    // Connect via CoreDeviceProxy → RSD (iOS 17+)
    auto cdp = CoreDeviceProxy::connect(provider)
                   .unwrap_or_else([](FfiError e) -> CoreDeviceProxy {
                       die("CoreDeviceProxy::connect (iOS 17+ required)", e);
                   });

    auto rsd_port = cdp.get_server_rsd_port().expect("get_server_rsd_port");
    auto adapter  = std::move(cdp).create_tcp_adapter().expect("create_tcp_adapter");
    auto stream   = adapter.connect(rsd_port).expect("adapter.connect");
    auto rsd      = RsdHandshake::from_socket(std::move(stream)).expect("RsdHandshake");
    auto rs       = RemoteServer::connect_rsd(adapter, rsd).expect("RemoteServer::connect_rsd");

    // Run all demos
    demo_device_info(rs);
    demo_application_listing(rs);
    demo_condition_inducer(rs);
    demo_network_monitor(rs);
    demo_sysmontap(rs);

    std::cout << "\nDone.\n";
    return 0;
}
