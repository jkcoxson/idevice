// Jackson Coxson

#include "idevice.h"
#include <arpa/inet.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

int main(int argc, char **argv) {
  // Initialize logger
  idevice_init_logger(Debug, Disabled, NULL);

  if (argc < 4) {
    fprintf(stderr,
            "Usage: %s <device_ip> <latitude> <longitude> [pairing_file]\n",
            argv[0]);
    return 1;
  }

  const char *device_ip = argv[1];
  double latitude = atof(argv[2]);
  double longitude = atof(argv[3]);
  const char *pairing_file = argc > 4 ? argv[4] : "pairing_file.plist";

  // Create the socket address
  struct sockaddr_in addr;
  memset(&addr, 0, sizeof(addr));
  addr.sin_family = AF_INET;
  addr.sin_port = htons(LOCKDOWN_PORT);
  inet_pton(AF_INET, device_ip, &addr.sin_addr);

  // Read pairing file
  IdevicePairingFile *pairing = NULL;
  IdeviceErrorCode err = idevice_pairing_file_read(pairing_file, &pairing);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to read pairing file: %d\n", err);
    return 1;
  }

  // Create TCP provider
  TcpProviderHandle *tcp_provider = NULL;
  err = idevice_tcp_provider_new((struct sockaddr *)&addr, pairing,
                                 "LocationSimCLI", &tcp_provider);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to create TCP provider: %d\n", err);
    idevice_pairing_file_free(pairing);
    return 1;
  }

  // Connect to CoreDeviceProxy
  CoreDeviceProxyHandle *core_device = NULL;
  err = core_device_proxy_connect_tcp(tcp_provider, &core_device);
  tcp_provider_free(tcp_provider);
  idevice_pairing_file_free(pairing);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to connect to CoreDeviceProxy: %d\n", err);
    return 1;
  }

  // Get server RSD port
  uint16_t rsd_port;
  err = core_device_proxy_get_server_rsd_port(core_device, &rsd_port);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to get server RSD port: %d\n", err);
    core_device_proxy_free(core_device);
    return 1;
  }

  // Create TCP adapter and connect to RSD port
  AdapterHandle *adapter = NULL;
  err = core_device_proxy_create_tcp_adapter(core_device, &adapter);
  core_device_proxy_free(core_device);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to create TCP adapter: %d\n", err);
    return 1;
  }

  err = adapter_connect(adapter, rsd_port);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to connect to RSD port: %d\n", err);
    adapter_free(adapter);
    return 1;
  }

  // Create XPC device
  XPCDeviceAdapterHandle *xpc_device = NULL;
  err = xpc_device_new(adapter, &xpc_device);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to create XPC device: %d\n", err);
    adapter_free(adapter);
    return 1;
  }

  // Get debug proxy service
  XPCServiceHandle *dvt_service = NULL;
  err = xpc_device_get_service(
      xpc_device, "com.apple.instruments.server.services.LocationSimulation",
      &dvt_service);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to get DVT service: %d\n", err);
    xpc_device_free(xpc_device);
    return 1;
  }

  // Reuse the adapter and connect to debug proxy port
  AdapterHandle *debug_adapter = NULL;
  err = xpc_device_adapter_into_inner(xpc_device, &debug_adapter);
  xpc_device_free(xpc_device);
  xpc_service_free(dvt_service);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to extract adapter: %d\n", err);
    return 1;
  }

  adapter_close(debug_adapter);
  err = adapter_connect(debug_adapter, dvt_service->port);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to connect to debug proxy port: %d\n", err);
    adapter_free(debug_adapter);
    return 1;
  }

  // Create RemoteServerClient
  RemoteServerAdapterHandle *remote_server = NULL;
  err = remote_server_adapter_new(debug_adapter, &remote_server);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to create remote server: %d\n", err);
    adapter_free(debug_adapter);
    return 1;
  }

  // Create LocationSimulationClient
  LocationSimulationAdapterHandle *location_sim = NULL;
  err = location_simulation_new(remote_server, &location_sim);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to create location simulation client: %d\n", err);
    remote_server_free(remote_server);
    return 1;
  }

  // Set location
  err = location_simulation_set(location_sim, latitude, longitude);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to set location: %d\n", err);
  } else {
    printf("Successfully set location to %.6f, %.6f\n", latitude, longitude);
  }

  printf("Press Enter to clear the simulated location...\n");
  getchar();

  // Clear location
  err = location_simulation_clear(location_sim);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to clear location: %d\n", err);
  } else {
    printf("Successfully cleared simulated location\n");
  }

  // Cleanup
  location_simulation_free(location_sim);
  remote_server_free(remote_server);

  printf("Done.\n");
  return 0;
}
