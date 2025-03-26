// Jackson Coxson

#include "idevice.h"
#include <arpa/inet.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

void print_service_details(XPCServiceHandle *service) {
  printf("  Service Details:\n");
  printf("    Entitlement: %s\n", service->entitlement);
  printf("    Port: %d\n", service->port);
  printf("    Uses Remote XPC: %s\n",
         service->uses_remote_xpc ? "true" : "false");
  printf("    Service Version: %lld\n", service->service_version);

  if (service->features_count > 0) {
    printf("    Features:\n");
    for (size_t i = 0; i < service->features_count; i++) {
      printf("      - %s\n", service->features[i]);
    }
  }
}

int main() {
  // Initialize logger
  idevice_init_logger(Debug, Disabled, NULL);

  // Create the socket address (replace with your device's IP)
  struct sockaddr_in addr;
  memset(&addr, 0, sizeof(addr));
  addr.sin_family = AF_INET;
  addr.sin_port = htons(LOCKDOWN_PORT);
  inet_pton(AF_INET, "10.7.0.2", &addr.sin_addr);

  // Read pairing file (replace with your pairing file path)
  IdevicePairingFile *pairing_file = NULL;
  IdeviceErrorCode err =
      idevice_pairing_file_read("pairing_file.plist", &pairing_file);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to read pairing file: %d\n", err);
    return 1;
  }

  /*****************************************************************
   * TCP Provider and CoreDeviceProxy Test
   *****************************************************************/
  printf("=== Testing TCP Provider and CoreDeviceProxy ===\n");

  // Create TCP provider
  TcpProviderHandle *tcp_provider = NULL;
  err = idevice_tcp_provider_new((struct sockaddr *)&addr, pairing_file,
                                 "CoreDeviceProxyTest", &tcp_provider);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to create TCP provider: %d\n", err);
    idevice_pairing_file_free(pairing_file);
    return 1;
  }

  // Connect to CoreDeviceProxy
  CoreDeviceProxyHandle *core_device = NULL;
  err = core_device_proxy_connect_tcp(tcp_provider, &core_device);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to connect to CoreDeviceProxy: %d\n", err);
    tcp_provider_free(tcp_provider);
    return 1;
  }
  tcp_provider_free(tcp_provider);

  // Get client parameters
  uint16_t mtu;
  char *address = NULL;
  char *netmask = NULL;
  err = core_device_proxy_get_client_parameters(core_device, &mtu, &address,
                                                &netmask);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to get client parameters: %d\n", err);
    core_device_proxy_free(core_device);
    return 1;
  }
  printf("Client Parameters:\n");
  printf("  MTU: %d\n", mtu);
  printf("  Address: %s\n", address);
  printf("  Netmask: %s\n", netmask);
  idevice_string_free(address);
  idevice_string_free(netmask);

  // Get server address
  char *server_address = NULL;
  err = core_device_proxy_get_server_address(core_device, &server_address);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to get server address: %d\n", err);
    core_device_proxy_free(core_device);
    return 1;
  }
  printf("Server Address: %s\n", server_address);
  idevice_string_free(server_address);

  // Get server RSD port
  uint16_t rsd_port;
  err = core_device_proxy_get_server_rsd_port(core_device, &rsd_port);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to get server RSD port: %d\n", err);
    core_device_proxy_free(core_device);
    return 1;
  }
  printf("Server RSD Port: %d\n", rsd_port);

  // Create TCP tunnel adapter
  AdapterHandle *adapter = NULL;
  err = core_device_proxy_create_tcp_adapter(core_device, &adapter);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to create TCP adapter: %d\n", err);
  } else {
    printf("Successfully created TCP tunnel adapter\n");
  }
  err = adapter_connect(adapter, rsd_port);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to connect to RSD port: %d\n", err);
  } else {
    printf("Successfully connected to RSD port\n");
  }

  /*****************************************************************
   * XPC Device Test
   *****************************************************************/
  printf("\n=== Testing XPC Device ===\n");

  // Create XPC device
  XPCDeviceAdapterHandle *xpc_device = NULL;
  err = xpc_device_new(adapter, &xpc_device);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to create XPC device: %d\n", err);
    core_device_proxy_free(core_device);
    return 1;
  }

  // List all services
  char **service_names = NULL;
  size_t service_count = 0;
  err =
      xpc_device_get_service_names(xpc_device, &service_names, &service_count);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to get service names: %d\n", err);
    xpc_device_free(xpc_device);
    return 1;
  }

  printf("Available Services (%zu):\n", service_count);
  for (size_t i = 0; i < service_count; i++) {
    printf("- %s\n", service_names[i]);

    // Get service details for each service
    XPCServiceHandle *service = NULL;
    err = xpc_device_get_service(xpc_device, service_names[i], &service);
    if (err == IdeviceSuccess) {
      print_service_details(service);
      xpc_service_free(service);
    } else {
      printf("  Failed to get service details: %d\n", err);
    }
  }
  xpc_device_free_service_names(service_names, service_count);

  // Test getting a specific service
  const char *test_service_name = "com.apple.internal.dt.remote.debugproxy";
  XPCServiceHandle *test_service = NULL;
  err = xpc_device_get_service(xpc_device, test_service_name, &test_service);
  if (err == IdeviceSuccess) {
    printf("\nSuccessfully retrieved service '%s':\n", test_service_name);
    print_service_details(test_service);
    xpc_service_free(test_service);
  } else {
    printf("\nFailed to get service '%s': %d\n", test_service_name, err);
  }

  /*****************************************************************
   * Adapter return
   *****************************************************************/
  AdapterHandle *adapter_return = NULL;
  err = xpc_device_adapter_into_inner(xpc_device, &adapter_return);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to extract adapter: %d\n", err);
  } else {
    printf("Successfully extracted adapter\n");
  }

  err = adapter_close(adapter_return);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to close adapter port: %d\n", err);
  } else {
    printf("Successfully closed adapter port\n");
  }

  /*****************************************************************
   * Cleanup
   *****************************************************************/
  adapter_free(adapter_return);

  printf("\nAll tests completed successfully!\n");
  return 0;
}
