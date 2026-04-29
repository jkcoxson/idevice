// Jackson Coxson

#pragma once
#include <idevice++/bindings.hpp>
#include <idevice++/dvt/remote_server.hpp>
#include <idevice++/result.hpp>
#include <memory>
#include <vector>

namespace IdeviceFFI {

using EnergyMonitorPtr =
    std::unique_ptr<EnergyMonitorHandle,
                    FnDeleter<EnergyMonitorHandle, energy_monitor_free>>;

struct EnergySample {
    uint32_t pid               = 0;
    int64_t  timestamp         = 0;
    double   total_energy      = 0.0;
    double   cpu_energy        = 0.0;
    double   gpu_energy        = 0.0;
    double   networking_energy = 0.0;
    double   display_energy    = 0.0;
    double   location_energy   = 0.0;
    double   appstate_energy   = 0.0;
};

class EnergyMonitor {
  public:
    static Result<EnergyMonitor, FfiError> create(RemoteServer& server);

    Result<void, FfiError>                     start_sampling(const std::vector<uint32_t>& pids);
    Result<void, FfiError>                     stop_sampling(const std::vector<uint32_t>& pids);
    Result<std::vector<EnergySample>, FfiError> sample_attributes(
        const std::vector<uint32_t>& pids);

    ~EnergyMonitor() noexcept                           = default;
    EnergyMonitor(EnergyMonitor&&) noexcept             = default;
    EnergyMonitor& operator=(EnergyMonitor&&) noexcept  = default;
    EnergyMonitor(const EnergyMonitor&)                 = delete;
    EnergyMonitor& operator=(const EnergyMonitor&)      = delete;

    EnergyMonitorHandle* raw() const noexcept { return handle_.get(); }
    static EnergyMonitor adopt(EnergyMonitorHandle* h) noexcept { return EnergyMonitor(h); }

  private:
    explicit EnergyMonitor(EnergyMonitorHandle* h) noexcept : handle_(h) {}
    EnergyMonitorPtr handle_{};
};

} // namespace IdeviceFFI
