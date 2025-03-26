// Jackson Coxson

#include "idevice.h"
#include <arpa/inet.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#define MAX_COMMAND_LENGTH 1024

void print_usage(const char *program_name) {
  printf("Usage: %s <device_ip> [pairing_file]\n", program_name);
  printf("Example: %s 10.0.0.1 pairing.plist\n", program_name);
}

int main(int argc, char **argv) {
  // Initialize logger
  idevice_init_logger(Debug, Disabled, NULL);

  if (argc < 2) {
    print_usage(argv[0]);
    return 1;
  }

  const char *device_ip = argv[1];
  const char *pairing_file = argc > 2 ? argv[2] : "pairing_file.plist";

  /*****************************************************************
   * CoreDeviceProxy Setup
   *****************************************************************/
  printf("=== Setting up CoreDeviceProxy ===\n");

  // Create socket address
  struct sockaddr_in addr;
  memset(&addr, 0, sizeof(addr));
  addr.sin_family = AF_INET;
  addr.sin_port = htons(LOCKDOWN_PORT);
  if (inet_pton(AF_INET, device_ip, &addr.sin_addr) != 1) {
    fprintf(stderr, "Invalid IP address\n");
    return 1;
  }

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
                                 "DebugProxyShell", &tcp_provider);
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
   * Get Debug Proxy Service
   *****************************************************************/
  printf("\n=== Getting Debug Proxy Service ===\n");

  XPCServiceHandle *debug_service = NULL;
  err = xpc_device_get_service(
      xpc_device, "com.apple.internal.dt.remote.debugproxy", &debug_service);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to get debug proxy service: %d\n", err);
    xpc_device_free(xpc_device);
    adapter_free(adapter);
    core_device_proxy_free(core_device);
    idevice_pairing_file_free(pairing);
    return 1;
  }
  printf("Debug Proxy Service Port: %d\n", debug_service->port);

  /*****************************************************************
   * Debug Proxy Setup
   *****************************************************************/
  printf("\n=== Setting up Debug Proxy ===\n");

  // Get the adapter back from XPC device
  AdapterHandle *debug_adapter = NULL;
  err = xpc_device_adapter_into_inner(xpc_device, &debug_adapter);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to extract adapter: %d\n", err);
    xpc_service_free(debug_service);
    xpc_device_free(xpc_device);
    core_device_proxy_free(core_device);
    idevice_pairing_file_free(pairing);
    return 1;
  }

  // Connect to debug proxy port
  err = adapter_connect(debug_adapter, debug_service->port);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to connect to debug proxy port: %d\n", err);
    adapter_free(debug_adapter);
    xpc_service_free(debug_service);
    core_device_proxy_free(core_device);
    idevice_pairing_file_free(pairing);
    return 1;
  }
  printf("Successfully connected to debug proxy port\n");

  // Create DebugProxyClient
  DebugProxyAdapterHandle *debug_proxy = NULL;
  err = debug_proxy_adapter_new(debug_adapter, &debug_proxy);
  if (err != IdeviceSuccess) {
    fprintf(stderr, "Failed to create debug proxy client: %d\n", err);
    adapter_free(debug_adapter);
    xpc_service_free(debug_service);
    core_device_proxy_free(core_device);
    idevice_pairing_file_free(pairing);
    return 1;
  }

  /*****************************************************************
   * Interactive Shell
   *****************************************************************/
  printf("\n=== Starting Interactive Debug Shell ===\n");
  printf("Type GDB debugserver commands or 'quit' to exit\n\n");

  char command[MAX_COMMAND_LENGTH];
  bool running = true;

  while (running) {
    printf("debug> ");
    fflush(stdout);

    if (fgets(command, sizeof(command), stdin) == NULL) {
      break;
    }

    // Remove newline
    command[strcspn(command, "\n")] = '\0';

    if (strcmp(command, "quit") == 0) {
      running = false;
      continue;
    }

    // Split command into name and arguments
    char *name = strtok(command, " ");
    char *args = strtok(NULL, "");

    // Create command
    DebugserverCommandHandle *cmd = NULL;
    if (args != NULL && args[0] != '\0') {
      // Split arguments
      char *argv[16] = {0};
      int argc = 0;
      char *token = strtok(args, " ");
      while (token != NULL && argc < 15) {
        argv[argc++] = token;
        token = strtok(NULL, " ");
      }

      cmd = debugserver_command_new(name, (const char **)argv, argc);
    } else {
      cmd = debugserver_command_new(name, NULL, 0);
    }

    if (cmd == NULL) {
      fprintf(stderr, "Failed to create command\n");
      continue;
    }

    // Send command
    char *response = NULL;
    err = debug_proxy_send_command(debug_proxy, cmd, &response);
    debugserver_command_free(cmd);

    if (err != IdeviceSuccess) {
      fprintf(stderr, "Command failed with error: %d\n", err);
      continue;
    }

    if (response != NULL) {
      printf("%s\n", response);
      idevice_string_free(response);
    } else {
      printf("(no response)\n");
    }

    // Read any additional responses
    while (true) {
      err = debug_proxy_read_response(debug_proxy, &response);
      if (err != IdeviceSuccess || response == NULL) {
        break;
      }
      printf("%s\n", response);
      idevice_string_free(response);
    }
  }

  /*****************************************************************
   * Cleanup
   *****************************************************************/
  debug_proxy_free(debug_proxy);
  xpc_service_free(debug_service);

  printf("\nDebug session ended\n");
  return 0;
}
