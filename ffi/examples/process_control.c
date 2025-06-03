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

  if (argc < 3) {
    fprintf(stderr, "Usage: %s <device_ip> <bundle_id> [pairing_file]\n",
            argv[0]);
    return 1;
  }

  const char *device_ip = argv[1];
  const char *bundle_id = argv[2];
  const char *pairing_file = argc > 3 ? argv[3] : "pairing_file.plist";

  // Create the socket address
  struct sockaddr_in addr;
  memset(&addr, 0, sizeof(addr));
  addr.sin_family = AF_INET;
  addr.sin_port = htons(LOCKDOWN_PORT);
  inet_pton(AF_INET, device_ip, &addr.sin_addr);

  // Read pairing file
  IdevicePairingFile *pairing = NULL;
  IdeviceFfiError *err = idevice_pairing_file_read(pairing_file, &pairing);
  if (err != NULL) {
    fprintf(stderr, "Failed to read pairing file: [%d] %s", err->code,
            err->message);
    idevice_error_free(err);
    return 1;
  }

  /*****************************************************************
   * CoreDeviceProxy Setup
   *****************************************************************/
  printf("=== Setting up CoreDeviceProxy ===\n");

  // Create TCP provider
  IdeviceProviderHandle *tcp_provider = NULL;
  err = idevice_tcp_provider_new((struct sockaddr *)&addr, pairing,
                                 "ProcessControlTest", &tcp_provider);
  if (err != NULL) {
    fprintf(stderr, "Failed to create TCP provider: [%d] %s", err->code,
            err->message);
    idevice_error_free(err);
    idevice_pairing_file_free(pairing);
    return 1;
  }

  // Connect to CoreDeviceProxy
  CoreDeviceProxyHandle *core_device = NULL;
  err = core_device_proxy_connect(tcp_provider, &core_device);
  if (err != NULL) {
    fprintf(stderr, "Failed to connect to CoreDeviceProxy: [%d] %s", err->code,
            err->message);
    idevice_error_free(err);
    idevice_provider_free(tcp_provider);
    idevice_pairing_file_free(pairing);
    return 1;
  }
  idevice_provider_free(tcp_provider);

  // Get server RSD port
  uint16_t rsd_port;
  err = core_device_proxy_get_server_rsd_port(core_device, &rsd_port);
  if (err != NULL) {
    fprintf(stderr, "Failed to get server RSD port: [%d] %s", err->code,
            err->message);
    idevice_error_free(err);
    core_device_proxy_free(core_device);
    idevice_pairing_file_free(pairing);
    return 1;
  }
  printf("Server RSD Port: %d\n", rsd_port);

  /*****************************************************************
   * Create TCP Tunnel Adapter
   *****************************************************************/
  printf("\n=== Creating TCP Tunnel Adapter ===\n");

  AdapterHandle *adapter = NULL;
  err = core_device_proxy_create_tcp_adapter(core_device, &adapter);
  if (err != NULL) {
    fprintf(stderr, "Failed to create TCP adapter: [%d] %s", err->code,
            err->message);
    idevice_error_free(err);
    core_device_proxy_free(core_device);
    idevice_pairing_file_free(pairing);
    return 1;
  }

  // Connect to RSD port
  AdapterStreamHandle *stream = NULL;
  err = adapter_connect(adapter, rsd_port, &stream);
  if (err != NULL) {
    fprintf(stderr, "Failed to connect to RSD port: [%d] %s", err->code,
            err->message);
    idevice_error_free(err);
    adapter_free(adapter);
    return 1;
  }

  RsdHandshakeHandle *handshake = NULL;
  err = rsd_handshake_new(stream, &handshake);
  if (err != NULL) {
    fprintf(stderr, "Failed to perform RSD handshake: [%d] %s", err->code,
            err->message);
    idevice_error_free(err);
    adapter_close(stream);
    adapter_free(adapter);
    return 1;
  }

  // Create RemoteServerClient
  RemoteServerHandle *remote_server = NULL;
  err = remote_server_connect_rsd(adapter, handshake, &remote_server);
  if (err != NULL) {
    fprintf(stderr, "Failed to create remote server: [%d] %s", err->code,
            err->message);
    idevice_error_free(err);
    adapter_free(adapter);
    rsd_handshake_free(handshake);
    return 1;
  }

  printf("\n=== Testing Process Control ===\n");

  // Create ProcessControlClient
  ProcessControlHandle *process_control = NULL;
  err = process_control_new(remote_server, &process_control);
  if (err != NULL) {
    fprintf(stderr, "Failed to create process control client: [%d] %s",
            err->code, err->message);
    idevice_error_free(err);
    remote_server_free(remote_server);
    return 1;
  }

  // Launch application
  uint64_t pid;
  err = process_control_launch_app(process_control, bundle_id, NULL, 0, NULL, 0,
                                   true, false, &pid);
  if (err != NULL) {
    fprintf(stderr, "Failed to launch app: [%d] %s", err->code, err->message);
    idevice_error_free(err);
    process_control_free(process_control);
    remote_server_free(remote_server);
    return 1;
  }
  printf("Successfully launched app with PID: %llu\n", pid);

  // Disable memory limits
  err = process_control_disable_memory_limit(process_control, pid);
  if (err != NULL) {
    fprintf(stderr, "Failed to disable memory limits: [%d] %s", err->code,
            err->message);
    idevice_error_free(err);
  } else {
    printf("Successfully disabled memory limits\n");
  }

  /*****************************************************************
   * Cleanup
   *****************************************************************/
  process_control_free(process_control);
  remote_server_free(remote_server);

  printf("\nAll tests completed successfully!\n");
  return 0;
}
