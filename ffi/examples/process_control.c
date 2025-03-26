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
  IdeviceErrorCode err = idevice_pairing_file_read(pairing_file, &pairing);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to read pairing file: %d\n", err);
    return 1;
  }

  /*****************************************************************
   * CoreDeviceProxy Setup
   *****************************************************************/
  printf("=== Setting up CoreDeviceProxy ===\n");

  // Create TCP provider
  TcpProviderHandle *tcp_provider = NULL;
  err = idevice_tcp_provider_new((struct sockaddr *)&addr, pairing,
                                 "ProcessControlTest", &tcp_provider);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to create TCP provider: %d\n", err);
    idevice_pairing_file_free(pairing);
    return 1;
  }

  // Connect to CoreDeviceProxy
  CoreDeviceProxyHandle *core_device = NULL;
  err = core_device_proxy_connect_tcp(tcp_provider, &core_device);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to connect to CoreDeviceProxy: %d\n", err);
    tcp_provider_free(tcp_provider);
    idevice_pairing_file_free(pairing);
    return 1;
  }
  tcp_provider_free(tcp_provider);

  // Get server RSD port
  uint16_t rsd_port;
  err = core_device_proxy_get_server_rsd_port(core_device, &rsd_port);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to get server RSD port: %d\n", err);
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
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to create TCP adapter: %d\n", err);
    core_device_proxy_free(core_device);
    idevice_pairing_file_free(pairing);
    return 1;
  }

  // Connect to RSD port
  err = adapter_connect(adapter, rsd_port);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to connect to RSD port: %d\n", err);
    adapter_free(adapter);
    core_device_proxy_free(core_device);
    idevice_pairing_file_free(pairing);
    return 1;
  }
  printf("Successfully connected to RSD port\n");

  /*****************************************************************
   * XPC Device Setup
   *****************************************************************/
  printf("\n=== Setting up XPC Device ===\n");

  XPCDeviceAdapterHandle *xpc_device = NULL;
  err = xpc_device_new(adapter, &xpc_device);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to create XPC device: %d\n", err);
    adapter_free(adapter);
    core_device_proxy_free(core_device);
    idevice_pairing_file_free(pairing);
    return 1;
  }

  /*****************************************************************
   * Get DVT Service
   *****************************************************************/
  printf("\n=== Getting Debug Proxy Service ===\n");

  XPCServiceHandle *dvt_service = NULL;
  err = xpc_device_get_service(xpc_device, "com.apple.instruments.dtservicehub",
                               &dvt_service);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to get DVT service: %d\n", err);
    xpc_device_free(xpc_device);
    return 1;
  }
  printf("Debug Proxy Service Port: %d\n", dvt_service->port);

  /*****************************************************************
   * Remote Server Setup
   *****************************************************************/
  printf("\n=== Setting up Remote Server ===\n");

  // Get the adapter back from XPC device
  AdapterHandle *debug_adapter = NULL;
  err = xpc_device_adapter_into_inner(xpc_device, &debug_adapter);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to extract adapter: %d\n", err);
    xpc_service_free(dvt_service);
    xpc_device_free(xpc_device);
    return 1;
  }

  // Connect to debug proxy port
  adapter_close(debug_adapter);
  err = adapter_connect(debug_adapter, dvt_service->port);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to connect to debug proxy port: %d\n", err);
    adapter_free(debug_adapter);
    xpc_service_free(dvt_service);
    return 1;
  }
  printf("Successfully connected to debug proxy port\n");

  // Create RemoteServerClient
  RemoteServerAdapterHandle *remote_server = NULL;
  err = remote_server_adapter_new(debug_adapter, &remote_server);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to create remote server: %d\n", err);
    adapter_free(debug_adapter);
    xpc_service_free(dvt_service);
    return 1;
  }

  /*****************************************************************
   * Process Control Test
   *****************************************************************/
  printf("\n=== Testing Process Control ===\n");

  // Create ProcessControlClient
  ProcessControlAdapterHandle *process_control = NULL;
  err = process_control_new(remote_server, &process_control);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to create process control client: %d\n", err);
    remote_server_free(remote_server);
    return 1;
  }

  // Launch application
  uint64_t pid;
  err = process_control_launch_app(process_control, bundle_id, NULL, 0, NULL, 0,
                                   true, false, &pid);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to launch app: %d\n", err);
    process_control_free(process_control);
    remote_server_free(remote_server);
    return 1;
  }
  printf("Successfully launched app with PID: %llu\n", pid);

  // Disable memory limits
  err = process_control_disable_memory_limit(process_control, pid);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to disable memory limits: %d\n", err);
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
