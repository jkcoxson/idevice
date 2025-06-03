// Jackson Coxson

#include "idevice.h"
#include <arpa/inet.h>
#include <inttypes.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

void print_usage(const char *program_name) {
  printf("Usage: %s <device_ip> <bundle_id> [pairing_file]\n", program_name);
  printf("Example: %s 10.0.0.1 com.example.app pairing.plist\n", program_name);
}

int main(int argc, char **argv) {
  // Initialize logger
  idevice_init_logger(Info, Disabled, NULL);

  if (argc < 3) {
    print_usage(argv[0]);
    return 1;
  }

  const char *device_ip = argv[1];
  const char *bundle_id = argv[2];
  const char *pairing_file = argc > 3 ? argv[3] : "pairing_file.plist";

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
  struct IdevicePairingFile *pairing = NULL;
  IdeviceFfiError *err = idevice_pairing_file_read(pairing_file, &pairing);
  if (err != NULL) {
    fprintf(stderr, "Failed to read pairing file: [%d] %s\n", err->code,
            err->message);
    return 1;
  }

  // Create TCP provider
  struct IdeviceProviderHandle *provider = NULL;
  err = idevice_tcp_provider_new((struct sockaddr *)&addr, pairing,
                                 "ProcessDebugTest", &provider);
  if (err != NULL) {
    fprintf(stderr, "Failed to create TCP provider: [%d] %s\n", err->code,
            err->message);
    idevice_pairing_file_free(pairing);
    return 1;
  }

  // Connect to CoreDeviceProxy
  struct CoreDeviceProxyHandle *core_device = NULL;
  err = core_device_proxy_connect(provider, &core_device);
  if (err != NULL) {
    fprintf(stderr, "Failed to connect to CoreDeviceProxy: [%d] %s", err->code,
            err->message);
    idevice_provider_free(provider);
    return 1;
  }
  idevice_provider_free(provider);

  // Get server RSD port
  uint16_t rsd_port;
  err = core_device_proxy_get_server_rsd_port(core_device, &rsd_port);
  if (err != NULL) {
    fprintf(stderr, "Failed to get server RSD port: [%d] %s", err->code,
            err->message);
    core_device_proxy_free(core_device);
    return 1;
  }
  printf("Server RSD Port: %d\n", rsd_port);

  /*****************************************************************
   * Create TCP Tunnel Adapter
   *****************************************************************/
  printf("\n=== Creating TCP Tunnel Adapter ===\n");

  struct AdapterHandle *adapter = NULL;
  err = core_device_proxy_create_tcp_adapter(core_device, &adapter);
  if (err != NULL) {
    fprintf(stderr, "Failed to create TCP adapter: [%d] %s", err->code,
            err->message);
    core_device_proxy_free(core_device);
    return 1;
  }

  // Connect to RSD port
  struct ReadWriteOpaque *rsd_stream = NULL;
  err = adapter_connect(adapter, rsd_port, &rsd_stream);
  if (err != NULL) {
    fprintf(stderr, "Failed to connect to RSD port: [%d] %s", err->code,
            err->message);
    adapter_free(adapter);
    return 1;
  }
  printf("Successfully connected to RSD port\n");

  adapter_pcap(adapter, "jit.pcap");

  /*****************************************************************
   * RSD Handshake
   *****************************************************************/
  printf("\n=== Performing RSD Handshake ===\n");

  struct RsdHandshakeHandle *rsd_handshake = NULL;
  err = rsd_handshake_new(rsd_stream, &rsd_handshake);
  if (err != NULL) {
    fprintf(stderr, "Failed to create RSD handshake: [%d] %s", err->code,
            err->message);
    adapter_free(adapter);
    return 1;
  }

  // Get services
  struct CRsdServiceArray *services = NULL;
  err = rsd_get_services(rsd_handshake, &services);
  if (err != NULL) {
    fprintf(stderr, "Failed to get RSD services: [%d] %s", err->code,
            err->message);
    rsd_handshake_free(rsd_handshake);
    adapter_free(adapter);
    return 1;
  }

  // Find debug proxy and process control services
  uint16_t debug_port = 0;
  uint16_t pc_port = 0;

  for (size_t i = 0; i < services->count; i++) {
    struct CRsdService *service = &services->services[i];
    if (strcmp(service->name, "com.apple.internal.dt.remote.debugproxy") == 0) {
      debug_port = service->port;
    } else if (strcmp(service->name, "com.apple.instruments.dtservicehub") ==
               0) {
      pc_port = service->port;
    }
  }

  rsd_free_services(services);

  if (debug_port == 0 || pc_port == 0) {
    fprintf(stderr, "Required services not found\n");
    adapter_free(adapter);
    return 1;
  }

  /*****************************************************************
   * Process Control - Launch App
   *****************************************************************/
  printf("\n=== Launching App ===\n");

  // Connect to process control port
  struct ReadWriteOpaque *pc_stream = NULL;
  err = adapter_connect(adapter, pc_port, &pc_stream);
  if (err != NULL) {
    fprintf(stderr, "Failed to connect to process control port: [%d] %s",
            err->code, err->message);
    idevice_error_free(err);
    adapter_free(adapter);
    return 1;
  }
  printf("Successfully connected to process control port\n");

  // Create RemoteServerClient
  struct RemoteServerHandle *remote_server = NULL;
  err = remote_server_new(pc_stream, &remote_server);
  if (err != NULL) {
    fprintf(stderr, "Failed to create remote server: [%d] %s", err->code,
            err->message);
    idevice_error_free(err);
    adapter_free(adapter);
    return 1;
  }

  // Create ProcessControlClient
  struct ProcessControlHandle *process_control = NULL;
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
    process_control_free(process_control);
    remote_server_free(remote_server);
    idevice_error_free(err);
    return 1;
  }
  printf("Successfully launched app with PID: %" PRIu64 "\n", pid);

  /*****************************************************************
   * Debug Proxy - Attach to Process
   *****************************************************************/
  printf("\n=== Attaching Debugger ===\n");

  // Connect to debug proxy port
  struct ReadWriteOpaque *debug_stream = NULL;
  err = adapter_connect(adapter, debug_port, &debug_stream);
  if (err != NULL) {
    fprintf(stderr, "Failed to connect to debug proxy port: [%d] %s", err->code,
            err->message);
    process_control_free(process_control);
    remote_server_free(remote_server);
    idevice_error_free(err);
    return 1;
  }
  printf("Successfully connected to debug proxy port\n");

  // Create DebugProxyClient
  struct DebugProxyHandle *debug_proxy = NULL;
  err = debug_proxy_connect_rsd(adapter, rsd_handshake, &debug_proxy);
  if (err != NULL) {
    fprintf(stderr, "Failed to create debug proxy client: [%d] %s", err->code,
            err->message);
    process_control_free(process_control);
    remote_server_free(remote_server);
    idevice_error_free(err);
    return 1;
  }

  // Send vAttach command with PID in hex
  char attach_command[64];
  snprintf(attach_command, sizeof(attach_command), "vAttach;%" PRIx64, pid);

  struct DebugserverCommandHandle *attach_cmd =
      debugserver_command_new(attach_command, NULL, 0);
  if (attach_cmd == NULL) {
    fprintf(stderr, "Failed to create attach command\n");
    debug_proxy_free(debug_proxy);
    process_control_free(process_control);
    remote_server_free(remote_server);
    idevice_error_free(err);
    return 1;
  }

  char *attach_response = NULL;
  err = debug_proxy_send_command(debug_proxy, attach_cmd, &attach_response);
  debugserver_command_free(attach_cmd);

  if (err != NULL) {
    fprintf(stderr, "Failed to attach to process: [%d] %s", err->code,
            err->message);
    idevice_error_free(err);
  } else if (attach_response != NULL) {
    printf("Attach response: %s\n", attach_response);
    idevice_string_free(attach_response);
  }

  // Send detach command
  struct DebugserverCommandHandle *detach_cmd =
      debugserver_command_new("D", NULL, 0);
  if (detach_cmd == NULL) {
    fprintf(stderr, "Failed to create detach command\n");
    idevice_error_free(err);
  } else {
    char *detach_response = NULL;
    err = debug_proxy_send_command(debug_proxy, detach_cmd, &detach_response);
    err = debug_proxy_send_command(debug_proxy, detach_cmd, &detach_response);
    err = debug_proxy_send_command(debug_proxy, detach_cmd, &detach_response);
    debugserver_command_free(detach_cmd);

    if (err != NULL) {
      fprintf(stderr, "Failed to detach from process: [%d] %s", err->code,
              err->message);
      idevice_error_free(err);
    } else if (detach_response != NULL) {
      printf("Detach response: %s\n", detach_response);
      idevice_string_free(detach_response);
    }
  }

  /*****************************************************************
   * Cleanup
   *****************************************************************/
  debug_proxy_free(debug_proxy);
  process_control_free(process_control);
  remote_server_free(remote_server);
  adapter_free(adapter);
  rsd_handshake_free(rsd_handshake);

  printf("\nDebug session completed\n");
  return 0;
}
