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
  IdeviceProviderHandle *tcp_provider = NULL;
  err = idevice_tcp_provider_new((struct sockaddr *)&addr, pairing,
                                 "LocationSimCLI", &tcp_provider);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to create TCP provider: %d\n", err);
    idevice_pairing_file_free(pairing);
    return 1;
  }

  // Connect to CoreDeviceProxy
  CoreDeviceProxyHandle *core_device = NULL;
  err = core_device_proxy_connect(tcp_provider, &core_device);
  idevice_provider_free(tcp_provider);
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

  // Connect to RSD port
  AdapterStreamHandle *stream = NULL;
  err = adapter_connect(adapter, rsd_port, &stream);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to connect to RSD port: %d\n", err);
    adapter_free(adapter);
    return 1;
  }

  RsdHandshakeHandle *handshake = NULL;
  err = rsd_handshake_new(stream, &handshake);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to perform RSD handshake: %d\n", err);
    adapter_close(stream);
    adapter_free(adapter);
    return 1;
  }

  // Create RemoteServerClient
  RemoteServerHandle *remote_server = NULL;
  err = remote_server_connect_rsd(adapter, handshake, &remote_server);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to create remote server: %d\n", err);
    adapter_free(adapter);
    rsd_handshake_free(handshake);
    return 1;
  }

  // Create LocationSimulationClient
  LocationSimulationHandle *location_sim = NULL;
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
