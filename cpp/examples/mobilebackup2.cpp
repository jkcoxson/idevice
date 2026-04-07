// Jackson Coxson
// MobileBackup2 example: backup a device to /tmp/idevice_backup_test/

#include <cstdio>
#include <cstring>
#include <fstream>
#include <idevice++/mobilebackup2.hpp>
#include <idevice++/provider.hpp>
#include <idevice++/usbmuxd.hpp>
#include <iostream>
#include <sys/stat.h>
#include <sys/statvfs.h>
#include <unistd.h>
#include <vector>

// ---- Filesystem-backed delegate callbacks ----

static uint64_t fs_get_free_disk_space(const std::string& path) {
    struct statvfs st{};
    if (statvfs(path.c_str(), &st) == 0) {
        return static_cast<uint64_t>(st.f_bavail) * static_cast<uint64_t>(st.f_frsize);
    }
    return 0;
}

static std::vector<uint8_t> fs_open_file_read(const std::string& path) {
    std::ifstream f(path, std::ios::binary | std::ios::ate);
    if (!f) {
        throw std::runtime_error("file not found: " + path);
    }
    auto size = f.tellg();
    f.seekg(0);
    std::vector<uint8_t> buf(static_cast<size_t>(size));
    f.read(reinterpret_cast<char*>(buf.data()), size);
    return buf;
}

// Track open files for writing
#include <map>
static std::map<std::string, std::ofstream> g_open_files;

static void                                 fs_create_file_write(const std::string& path) {
    g_open_files[path] = std::ofstream(path, std::ios::binary | std::ios::trunc);
}

static void fs_write_chunk(const std::string& path, const uint8_t* data, size_t len) {
    auto it = g_open_files.find(path);
    if (it != g_open_files.end()) {
        it->second.write(reinterpret_cast<const char*>(data), static_cast<std::streamsize>(len));
    }
}

static void fs_close_file(const std::string& path) {
    auto it = g_open_files.find(path);
    if (it != g_open_files.end()) {
        it->second.close();
        g_open_files.erase(it);
    }
}

static void fs_create_dir_all(const std::string& path) {
    // Simple recursive mkdir
    std::string current;
    for (char c : path) {
        current += c;
        if (c == '/') {
            mkdir(current.c_str(), 0755);
        }
    }
    mkdir(current.c_str(), 0755);
}

static void fs_remove(const std::string& path) {
    struct stat st{};
    if (stat(path.c_str(), &st) != 0) {
        return;
    }
    if (S_ISDIR(st.st_mode)) {
        // Simple non-recursive for now
        rmdir(path.c_str());
    } else {
        unlink(path.c_str());
    }
}

static void fs_rename(const std::string& from, const std::string& to) {
    ::rename(from.c_str(), to.c_str());
}

static void fs_copy(const std::string& src, const std::string& dst) {
    struct stat st{};
    if (stat(src.c_str(), &st) == 0 && S_ISDIR(st.st_mode)) {
        mkdir(dst.c_str(), 0755);
    } else {
        auto          data = fs_open_file_read(src);
        std::ofstream f(dst, std::ios::binary);
        f.write(reinterpret_cast<const char*>(data.data()),
                static_cast<std::streamsize>(data.size()));
    }
}

static bool fs_exists(const std::string& path) {
    struct stat st{};
    return stat(path.c_str(), &st) == 0;
}

static bool fs_is_dir(const std::string& path) {
    struct stat st{};
    return stat(path.c_str(), &st) == 0 && S_ISDIR(st.st_mode);
}

static void progress_callback(uint64_t bytes_done, uint64_t bytes_total, double overall) {
    if (bytes_total > 0) {
        double pct = static_cast<double>(bytes_done) / static_cast<double>(bytes_total) * 100.0;
        fprintf(stderr,
                "\r  %.1f%%  %.1f MB / %.1f MB",
                pct,
                static_cast<double>(bytes_done) / (1024.0 * 1024.0),
                static_cast<double>(bytes_total) / (1024.0 * 1024.0));
    } else if (overall > 0) {
        fprintf(stderr, "\r  %.1f%%", overall);
    }
}

int main() {
    idevice_init_logger(Warn, Disabled, NULL);

    // Connect to first USB device
    auto u = IdeviceFFI::UsbmuxdConnection::default_new(0).expect("failed to connect to usbmuxd");
    auto devices = u.get_devices().expect("failed to get devices");
    if (devices.empty()) {
        std::cerr << "No devices connected\n";
        return 1;
    }

    auto& dev  = devices[0];
    auto  udid = dev.get_udid();
    if (udid.is_none()) {
        std::cerr << "No UDID\n";
        return 1;
    }
    auto id = dev.get_id();
    if (id.is_none()) {
        std::cerr << "No ID\n";
        return 1;
    }

    std::string udid_str = udid.unwrap();
    std::cout << "Device: " << udid_str << "\n";

    IdeviceFFI::UsbmuxdAddr addr = IdeviceFFI::UsbmuxdAddr::default_new();
    auto                    prov = IdeviceFFI::Provider::usbmuxd_new(
                                       std::move(addr), 0, udid_str, id.unwrap(), "mobilebackup2_test")
                                       .expect("Failed to create provider");

    // Connect to mobilebackup2
    auto                    client =
        IdeviceFFI::MobileBackup2::connect(prov).expect("Failed to connect to mobilebackup2");

    // Set up delegate callbacks
    IdeviceFFI::BackupDelegateCallbacks delegate;
    delegate.get_free_disk_space = fs_get_free_disk_space;
    delegate.open_file_read      = fs_open_file_read;
    delegate.create_file_write   = fs_create_file_write;
    delegate.write_chunk         = fs_write_chunk;
    delegate.close_file          = fs_close_file;
    delegate.create_dir_all      = fs_create_dir_all;
    delegate.remove              = fs_remove;
    delegate.rename              = fs_rename;
    delegate.copy                = fs_copy;
    delegate.exists              = fs_exists;
    delegate.is_dir              = fs_is_dir;
    delegate.on_progress         = progress_callback;

    std::string backup_dir       = "/tmp/idevice_backup_test/";
    fs_create_dir_all(backup_dir);

    // Backup
    std::cout << "Starting backup...\n";
    auto backup_result =
        client.backup(backup_dir, IdeviceFFI::Some(udid_str), IdeviceFFI::None, delegate);

    match_result(
        backup_result,
        response,
        {
            fprintf(stderr, "\n");
            std::cout << "Backup complete.\n";
            if (response) {
                plist_free(response);
            }
        },
        e,
        {
            fprintf(stderr, "\n");
            std::cerr << "Backup failed: " << e.message << "\n";
            return 1;
        });

    client.disconnect();
    std::cout << "Done.\n";
    return 0;
}
